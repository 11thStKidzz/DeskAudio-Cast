@echo off
if not defined DESKAUDIO_VSDEVCMD set "DESKAUDIO_VSDEVCMD=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat"
if not exist "%DESKAUDIO_VSDEVCMD%" exit /b 2
call "%DESKAUDIO_VSDEVCMD%" -arch=x64 -host_arch=x64 >nul
if errorlevel 1 exit /b %errorlevel%
"%USERPROFILE%\.cargo\bin\cargo.exe" %*
exit /b %errorlevel%
