Unicode true
!include "MUI2.nsh"
!include "x64.nsh"

!ifndef PLUGIN_DLL
  !error "Define PLUGIN_DLL with the path to the release DLL"
!endif
!ifndef MANIFEST
  !error "Define MANIFEST with the path to manifest.json"
!endif
!ifndef LICENSE_FILE
  !error "Define LICENSE_FILE with the path to LICENSE"
!endif
!ifndef NOTICES
  !error "Define NOTICES with the path to the third-party notices"
!endif
!ifndef OUTPUT_FILE
  !error "Define OUTPUT_FILE with the path to the generated installer"
!endif

Name "RTSPS Secure for OBS"
OutFile "${OUTPUT_FILE}"
InstallDir "$APPDATA\obs-studio\plugins\rtsps_secure"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show
BrandingText "RTSPS Secure for OBS"
VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "RTSPS Secure for OBS"
VIAddVersionKey "ProductVersion" "0.1.0"
VIAddVersionKey "CompanyName" "orozcorgint"
VIAddVersionKey "FileDescription" "RTSPS Secure installer for OBS Studio"
VIAddVersionKey "FileVersion" "0.1.0.0"
VIAddVersionKey "LegalCopyright" "Copyright (c) 2026 orozcorgint; MIT License"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_LINK "Project documentation"
!define MUI_FINISHPAGE_LINK_LOCATION "https://github.com/orozcorgint/obs-rtsps-secure#readme"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

!macro CheckOBSClosed FUNCTION_NAME
Function ${FUNCTION_NAME}
  ; Get-Process also finds portable and background instances. Never terminate OBS.
  nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -Command "if (Get-Process obs64 -ErrorAction SilentlyContinue) { exit 9 } else { exit 0 }"'
  Pop $0
  Pop $1
  StrCmp $0 "0" closed
  StrCmp $0 "9" running failed
running:
  IfSilent silent_running
  MessageBox MB_OK|MB_ICONSTOP "Close every OBS window and try again. OBS will not be closed automatically."
  Abort
silent_running:
  SetErrorLevel 9
  Abort
failed:
  IfSilent silent_failed
  MessageBox MB_OK|MB_ICONSTOP "Could not determine whether OBS is running. The plugin was not changed."
  Abort
silent_failed:
  SetErrorLevel 10
  Abort
closed:
FunctionEnd
!macroend

!ifndef TEST_NO_GUARD
!insertmacro CheckOBSClosed EnsureOBSClosed
!insertmacro CheckOBSClosed un.EnsureOBSClosed
!endif

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "This installer requires 64-bit Windows and 64-bit OBS Studio 33."
    Abort
  ${EndIf}
!ifndef TEST_SANDBOX
  ; Fixed destination: do not allow /D= or another install to redirect writes.
  SetShellVarContext all
  StrCpy $INSTDIR "$APPDATA\obs-studio\plugins\rtsps_secure"
!endif
!ifndef TEST_NO_GUARD
  Call EnsureOBSClosed
!endif
FunctionEnd

Section "Install"
!ifndef TEST_NO_GUARD
  Call EnsureOBSClosed
!endif
!ifndef TEST_SANDBOX
  StrCpy $3 "$APPDATA\obs-studio\plugin-backups\rtsps_secure"
!else
  StrCpy $3 "$INSTDIR\_test_backups"
!endif
  ; Preserve an existing plugin DLL; refuse to replace it if the backup fails.
  IfFileExists "$INSTDIR\rtsps_secure.dll" 0 no_backup
  CreateDirectory "$3"
  System::Call 'kernel32::GetTickCount64() l.r1'
  System::Call 'kernel32::GetCurrentProcessId() i.r2'
  StrCpy $4 "$3\rtsps_secure-$1-$2.dll"
  IfFileExists "$4" backup_failed
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\rtsps_secure.dll" "$4"
  IfErrors backup_failed
  IfFileExists "$4" no_backup backup_failed
backup_failed:
  IfSilent silent_backup_failed
  MessageBox MB_OK|MB_ICONSTOP "Could not back up the existing DLL. The plugin was not updated."
  Abort
silent_backup_failed:
  SetErrorLevel 11
  Abort
no_backup:
  SetOutPath "$INSTDIR"
  SetOverwrite on
  ClearErrors
  File /oname=rtsps_secure.dll "${PLUGIN_DLL}"
  IfErrors install_failed
  File /oname=LICENSE.txt "${LICENSE_FILE}"
  File /oname=THIRD_PARTY_NOTICES.txt "${NOTICES}"
  SetOutPath "$INSTDIR\data"
  File /oname=manifest.json "${MANIFEST}"
  IfErrors install_failed
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  IfErrors install_failed
!ifndef TEST_SANDBOX
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "DisplayName" "RTSPS Secure for OBS"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "Publisher" "orozcorgint"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "DisplayVersion" "0.1.0"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS" "NoRepair" 1
!endif
  Goto installed
install_failed:
  IfSilent silent_install_failed
  MessageBox MB_OK|MB_ICONSTOP "The installation did not complete. If an earlier version existed, its DLL was backed up."
  Abort
silent_install_failed:
  SetErrorLevel 12
  Abort
installed:
SectionEnd

Function un.onInit
!ifndef TEST_SANDBOX
  SetShellVarContext all
  StrCpy $INSTDIR "$APPDATA\obs-studio\plugins\rtsps_secure"
!endif
!ifndef TEST_NO_GUARD
  Call un.EnsureOBSClosed
!endif
FunctionEnd

Section "Uninstall"
!ifndef TEST_NO_GUARD
  Call un.EnsureOBSClosed
!endif
  Delete "$INSTDIR\rtsps_secure.dll"
  Delete "$INSTDIR\data\manifest.json"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR\data"
  RMDir "$INSTDIR"
!ifndef TEST_SANDBOX
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RTSPSSecureOBS"
!endif
  ; Deliberately leave scenes, plugin certificates, backups, and unrelated files.
SectionEnd
