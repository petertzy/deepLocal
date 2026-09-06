@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-dev.ps1" %*
set "DEELOCAL_EXIT_CODE=%ERRORLEVEL%"
if not "%DEELOCAL_EXIT_CODE%"=="0" echo deepLocal launcher failed with exit code %DEELOCAL_EXIT_CODE%.
exit /b %DEELOCAL_EXIT_CODE%
