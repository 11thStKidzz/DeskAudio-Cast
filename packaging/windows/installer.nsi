; Current-user app install. No driver, firewall rule or runtime download.
; An absent VC++ runtime is installed ONLY with explicit consent and Microsoft UI.
Unicode true
RequestExecutionLevel user
SetCompressor /SOLID zlib
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "FileFunc.nsh"
!include "WordFunc.nsh"
!insertmacro VersionCompare
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
!define MUI_PAGE_HEADER_TEXT "Microsoft WebView2 Fixed Runtime"
!define MUI_PAGE_HEADER_SUBTEXT "Separate Microsoft Runtime terms / 独立微软运行时条款"
!insertmacro MUI_PAGE_LICENSE "${PAYLOAD}\legal\native\WebView2-Fixed-Runtime-EULA.txt"
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
FunctionEnd

Function VCRuntimeReady
  StrCpy $R9 0
  ; x86 installer inspects x64 file versions without loading these DLLs.
  IfFileExists "$WINDIR\Sysnative\vcruntime140.dll" 0 vc_check_done
  IfFileExists "$WINDIR\Sysnative\vcruntime140_1.dll" 0 vc_check_done
  ${GetFileVersion} "$WINDIR\Sysnative\vcruntime140.dll" $0
  ${VersionCompare} "$0" "${VC_MIN_VERSION}" $1
  ${If} $1 == 2
    Goto vc_check_done
  ${EndIf}
  ${GetFileVersion} "$WINDIR\Sysnative\vcruntime140_1.dll" $0
  ${VersionCompare} "$0" "${VC_MIN_VERSION}" $1
  ${If} $1 != 2
    StrCpy $R9 1
  ${EndIf}
  vc_check_done:
FunctionEnd

Function EnsureVCRuntime
  Call VCRuntimeReady
  ${If} $R9 == 1
    Return
  ${EndIf}
  ; Silent install never silently installs a system dependency or requests UAC.
  IfSilent vc_cancel
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "需要 Microsoft VC++ x64 ${VC_MIN_VERSION} 或更新版本。安装包已附带微软官方离线安装器。是否打开它？可能需要管理员确认；请在微软界面查看并接受其许可。不联网下载，不自动重启。$\r$\nMicrosoft VC++ x64 ${VC_MIN_VERSION} or later is required. Open the included official offline installer? Administrator approval may be required. Review and accept Microsoft's license in its own UI. No download or automatic restart." IDYES vc_install IDNO vc_cancel
  vc_install:
    InitPluginsDir
    File /oname=$PLUGINSDIR\vc_redist.x64.exe "${VC_REDIST}"
    ClearErrors
    ExecWait '"$PLUGINSDIR\vc_redist.x64.exe" /install /norestart /log "$TEMP\DeskAudioCast-vcredist.log"' $2
    IfErrors vc_failed
    ${If} $2 != 0
    ${AndIf} $2 != 3010
    ${AndIf} $2 != 1638
      Goto vc_failed
    ${EndIf}
    ; An installer exit code alone is NOT proof the required runtime exists.
    Call VCRuntimeReady
    ${If} $R9 != 1
      Goto vc_failed
    ${EndIf}
    ${If} $2 == 3010
      MessageBox MB_ICONINFORMATION "微软运行库提示需要重启，请保存工作后自行重启。 / Microsoft Runtime requests a restart. Save your work and restart when convenient."
    ${EndIf}
    Return
  vc_failed:
    MessageBox MB_ICONSTOP "VC++ x64 运行库安装未完成或版本检查失败，停止安装。请检查微软安装器和日志后重试。 / VC++ x64 installation or version verification failed; setup stopped. Check the Microsoft installer and log, then retry.$\r$\n$TEMP\DeskAudioCast-vcredist.log$\r$\nhttps://learn.microsoft.com/cpp/windows/latest-supported-vc-redist"
    SetErrorLevel 1603
    Abort
  vc_cancel:
    SetErrorLevel 1602
    Abort
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
  Call EnsureVCRuntime
  !include "${INSTALL_FILES}"
  ; Microsoft requires renderer AppContainer RX access for unpackaged Windows 10.
  ; Grant only these read/execute ACLs on our runtime subtree, never system paths.
  ${IfNot} ${AtLeastWin11}
    nsExec::ExecToStack '"$WINDIR\Sysnative\icacls.exe" "$INSTDIR\WebView2Fixed" /grant "*S-1-15-2-2:(OI)(CI)(RX)" "*S-1-15-2-1:(OI)(CI)(RX)" /T /C /Q'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_ICONSTOP "内置运行时读取权限配置失败，安装停止。请重新安装或联系支持。 / Bundled Runtime read permissions could not be configured; setup stopped. Reinstall or contact support."
      SetErrorLevel 1603
      Abort
    ${EndIf}
  ${EndIf}
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
