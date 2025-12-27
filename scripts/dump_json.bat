@echo off
setlocal enabledelayedexpansion

if "%~1"=="" (
  echo Usage: dump_json.bat ^<base_name^>
  echo Example: dump_json.bat branch
  exit /b 2
)

set "BASE=%~1"
set "ROOT=railML\IS NEST view"
set "INPUT_A=%ROOT%\%BASE%"
set "INPUT_B=%ROOT%\%BASE%_import"
set "OUTPUT_A=%INPUT_A%.json"
set "OUTPUT_B=%INPUT_B%.json"

set "JUNCTION_EXE="
if exist "target-windows\debug\junction.exe" set "JUNCTION_EXE=target-windows\debug\junction.exe"
if "%JUNCTION_EXE%"=="" if exist "target\debug\junction.exe" set "JUNCTION_EXE=target\debug\junction.exe"

if "%JUNCTION_EXE%"=="" (
  echo junction.exe not found. Build first with: cargo build
  exit /b 1
)

if not exist "%INPUT_A%" (
  echo Missing "%INPUT_A%"
  exit /b 1
)

if not exist "%INPUT_B%" (
  echo Missing "%INPUT_B%"
  exit /b 1
)

"%JUNCTION_EXE%" --dump-json "%INPUT_A%" "%OUTPUT_A%"
if errorlevel 1 exit /b %errorlevel%

"%JUNCTION_EXE%" --dump-json "%INPUT_B%" "%OUTPUT_B%"
if errorlevel 1 exit /b %errorlevel%

echo Wrote "%OUTPUT_A%"
echo Wrote "%OUTPUT_B%"
echo.
fc "%OUTPUT_A%" "%OUTPUT_B%"
