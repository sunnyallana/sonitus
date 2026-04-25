@echo off
rem Tiny wrapper so users in cmd.exe can run the dev launcher without
rem typing the full PowerShell invocation. Forwards all arguments.

setlocal
set "SCRIPT_DIR=%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%run-dev.ps1" %*
exit /b %ERRORLEVEL%
