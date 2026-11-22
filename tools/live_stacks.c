/* Read-only diagnostic for a hung 64-bit process. Each thread is suspended
 * briefly only while its stack addresses are walked; it is resumed before
 * formatting or printing. No dump, application data or credentials are saved.
 * Build: gcc -O2 -o live_stacks.exe live_stacks.c -ldbghelp
 */
#define _WIN32_WINNT 0x0600
#include <windows.h>
#include <tlhelp32.h>
#include <dbghelp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_FRAMES 32

int main(int argc, char **argv)
{
    if (argc < 2) {
        fprintf(stderr, "usage: live_stacks PID [TID ...]\n");
        return 2;
    }
    DWORD pid = strtoul(argv[1], NULL, 10);
    HANDLE process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (!process) {
        fprintf(stderr, "OpenProcess: %lu\n", GetLastError());
        return 1;
    }
    SymSetOptions(SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME | SYMOPT_FAIL_CRITICAL_ERRORS | SYMOPT_NO_PROMPTS);
    if (!SymInitialize(process, "C:\\Program Files\\obs-studio\\bin\\64bit;C:\\Program Files\\obs-studio\\core\\obs-ffmpeg", TRUE)) {
        fprintf(stderr, "SymInitialize: %lu\n", GetLastError());
        CloseHandle(process);
        return 1;
    }
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snapshot == INVALID_HANDLE_VALUE) {
        fprintf(stderr, "Thread snapshot: %lu\n", GetLastError());
        SymCleanup(process); CloseHandle(process); return 1;
    }
    THREADENTRY32 entry = { .dwSize = sizeof(entry) };
    for (BOOL found = Thread32First(snapshot, &entry); found; found = Thread32Next(snapshot, &entry)) {
        if (entry.th32OwnerProcessID != pid) continue;
        if (argc > 2) {
            BOOL wanted = FALSE;
            for (int i = 2; i < argc; ++i) if (entry.th32ThreadID == strtoul(argv[i], NULL, 10)) wanted = TRUE;
            if (!wanted) continue;
        }
        HANDLE thread = OpenThread(THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION | THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID);
        if (!thread) continue;
        DWORD64 addresses[MAX_FRAMES] = {0};
        int count = 0;
        DWORD suspended = SuspendThread(thread);
        if (suspended != (DWORD)-1) {
            CONTEXT ctx = {0};
            ctx.ContextFlags = CONTEXT_FULL;
            if (GetThreadContext(thread, &ctx)) {
                STACKFRAME64 frame = {0};
                frame.AddrPC.Offset = ctx.Rip; frame.AddrPC.Mode = AddrModeFlat;
                frame.AddrFrame.Offset = ctx.Rbp; frame.AddrFrame.Mode = AddrModeFlat;
                frame.AddrStack.Offset = ctx.Rsp; frame.AddrStack.Mode = AddrModeFlat;
                addresses[count++] = ctx.Rip;
                while (count < MAX_FRAMES && StackWalk64(IMAGE_FILE_MACHINE_AMD64, process, thread, &frame, &ctx,
                        NULL, SymFunctionTableAccess64, SymGetModuleBase64, NULL) && frame.AddrPC.Offset) {
                    addresses[count++] = frame.AddrPC.Offset;
                }
            }
            ResumeThread(thread);
        }
        CloseHandle(thread);
        if (!count) continue;
        printf("\nTHREAD %lu (%d frames)\n", entry.th32ThreadID, count);
        for (int i = 0; i < count; ++i) {
            char symbuf[sizeof(SYMBOL_INFO) + MAX_SYM_NAME] = {0};
            PSYMBOL_INFO symbol = (PSYMBOL_INFO)symbuf;
            symbol->SizeOfStruct = sizeof(SYMBOL_INFO); symbol->MaxNameLen = MAX_SYM_NAME - 1;
            DWORD64 offset = 0;
            IMAGEHLP_MODULE64 mod = {0}; mod.SizeOfStruct = sizeof(mod);
            BOOL has_module = SymGetModuleInfo64(process, addresses[i], &mod);
            BOOL has_symbol = SymFromAddr(process, addresses[i], &offset, symbol);
            printf("  %02d %016llx %s!%s+0x%llx\n", i, (unsigned long long)addresses[i],
                has_module ? mod.ModuleName : "?", has_symbol ? symbol->Name : "?",
                (unsigned long long)(has_symbol ? offset : (has_module ? addresses[i] - mod.BaseOfImage : 0)));
        }
        fflush(stdout);
    }
    CloseHandle(snapshot); SymCleanup(process); CloseHandle(process);
    return 0;
}
