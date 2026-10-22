; Current-user LOCAL TEST installer. No driver, firewall rule, runtime download or elevation.
Unicode true
RequestExecutionLevel user
SetCompressor /SOLID zlib
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\DeskAudioCastLocalTest"
Name "桌面声渡 / DeskAudio Cast"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\DeskAudioCast"
InstallDirRegKey HKCU "${KEY}" "InstallLocation"
BrandingText "DeskAudio Cast · 11thStKidzz"
Icon "${ICON}"
UninstallIcon "${ICON}"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "DeskAudio Cast"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "DeskAudio Cast current-user installer"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PAYLOAD}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "本软件需要 Windows x64。 / Windows x64 is required."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP "需要 Windows 10 2004（19041）或更新版本。 / Windows 10 build 19041 or later is required."
    Abort
  ${EndIf}
  SetRegView 64
  ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuildNumber"
  ; Exact build check uses the string value, not a DWORD conversion by RegRead.
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuildNumber"
  IntCmp $0 19041 build_ok build_old build_ok
  build_old:
    MessageBox MB_ICONSTOP "需要 Windows 10 2004（19041）或更新版本。 / Windows 10 build 19041 or later is required."
    Abort
  build_ok:
  ; This NSIS stub is x86: inspect the x64 system directory, never load an x64 DLL.
  IfFileExists "$WINDIR\Sysnative\vcruntime140.dll" 0 missing_vc
  IfFileExists "$WINDIR\Sysnative\vcruntime140_1.dll" vc_ok missing_vc
  missing_vc:
    MessageBox MB_ICONSTOP "缺少 Microsoft VC++ x64 运行库。 / Microsoft VC++ x64 Runtime is required.$\r$\nhttps://learn.microsoft.com/cpp/windows/latest-supported-vc-redist$\r$\n本程序不会后台安装依赖。 / No automatic installation."
    Abort
  vc_ok:
  ; Microsoft documented Evergreen registration; check both installation scopes.
  SetRegView 32
  ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  SetRegView 64
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    MessageBox MB_ICONSTOP "缺少 Microsoft WebView2 Runtime。 / Microsoft WebView2 Runtime is required.$\r$\nhttps://developer.microsoft.com/microsoft-edge/webview2/$\r$\n本程序不会自动下载安装。 / No automatic installation."
    Abort
  ${EndIf}
FunctionEnd

Section "安装"
  SetShellVarContext current
  SetRegView 64
  ; Never force-close a playing app or replace a locked binary.
  IfFileExists "$INSTDIR\DeskAudioCast.exe" 0 install_files
  ClearErrors
  FileOpen $0 "$INSTDIR\DeskAudioCast.exe" a
  IfErrors in_use
  FileClose $0
  Goto install_files
  in_use:
    MessageBox MB_ICONSTOP "该目录的程序正在使用，请先自行退出。 / The app in this directory is in use. Please quit it first.$\r$\n不会强制停止播放。 / Playback will not be force-stopped."
    Abort
  install_files:
  !include "${INSTALL_FILES}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\桌面声渡.lnk" "$INSTDIR\DeskAudioCast.exe"
  CreateShortcut "$DESKTOP\桌面声渡.lnk" "$INSTDIR\DeskAudioCast.exe"
  WriteRegStr HKCU "${KEY}" "DisplayName" "桌面声渡 / DeskAudio Cast"
  WriteRegStr HKCU "${KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "${KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  ClearErrors
  FileOpen $0 "$INSTDIR\DeskAudioCast.exe" a
  IfErrors uninstall_in_use
  FileClose $0
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DeskAudioCast"
  ${If} $0 == '$\"$INSTDIR\DeskAudioCast.exe$\" --background'
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DeskAudioCast"
  ${EndIf}
  !include "${UNINSTALL_FILES}"
  Delete "$SMPROGRAMS\桌面声渡.lnk"
  Delete "$DESKTOP\桌面声渡.lnk"
  DeleteRegKey HKCU "${KEY}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  ; Preferences, logs and user-added files are deliberately preserved.
  Goto uninstall_done
  uninstall_in_use:
    MessageBox MB_ICONSTOP "请先从托盘退出本软件，再卸载。 / Quit the app from its tray before uninstalling.$\r$\n不会强制停止播放。 / Playback will not be force-stopped."
    Abort
  uninstall_done:
SectionEnd
