@echo off
setlocal
set "_VKIT_PATH=%PATH%"
set "PATH="
set "Path="
set "Path=%_VKIT_PATH%"
set "_VKIT_PATH="
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0build\windows\Build-Native.ps1" %*
exit /b %ERRORLEVEL%
