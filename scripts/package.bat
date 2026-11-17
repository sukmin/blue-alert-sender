@echo off
chcp 65001 >nul
setlocal EnableExtensions

rem ============================================================
rem  릴리스 빌드 후 배포 폴더(dist\blue-alert-sender)와 zip 을 만듭니다.
rem
rem  사용법: package.bat [/nobuild]
rem    /nobuild : cargo build 를 건너뛰고 target\release\blue-alert-sender.exe 를 그대로 사용
rem  결과:
rem    dist\blue-alert-sender\blue-alert-sender.exe
rem    dist\blue-alert-sender\사용법.txt (첫 줄에 버전), README.md
rem    dist\blue-alert-sender-<버전>-win64.zip
rem  버전은 Cargo.toml 의 version 입니다.
rem ============================================================

cd /d "%~dp0.."
set "NAME=blue-alert-sender"
set "DIST=dist\%NAME%"
set "EXE=target\release\%NAME%.exe"

set "VER="
for /f "tokens=2 delims== " %%V in ('findstr /b /c:"version" Cargo.toml') do if not defined VER set "VER=%%~V"
if not defined VER set "VER=dev"
set "ZIP=dist\%NAME%-%VER%-win64.zip"

echo %NAME% %VER% 패키징
echo.

if /I not "%~1"=="/nobuild" (
    echo [1/3] cargo build --release
    cargo build --release
    if errorlevel 1 (
        echo [오류] 빌드에 실패했습니다.
        exit /b 1
    )
)

if not exist "%EXE%" (
    echo [오류] %EXE% 가 없습니다. 먼저 빌드하세요.
    exit /b 1
)

echo [2/3] 배포 폴더 구성: %DIST%
if exist "%DIST%" rd /s /q "%DIST%"
mkdir "%DIST%"
copy /Y "%EXE%" "%DIST%\" >nul
copy /Y "scripts\dist-readme.txt" "%DIST%\사용법.txt" >nul
copy /Y "README.md" "%DIST%\" >nul
rem 사용법.txt 의 {VERSION} 을 실제 버전으로 바꿉니다.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem '%DIST%\*.txt' | ForEach-Object { $t = [IO.File]::ReadAllText($_.FullName); [IO.File]::WriteAllText($_.FullName, $t.Replace('{VERSION}', '%VER%'), (New-Object Text.UTF8Encoding $false)) }"

echo [3/3] 압축: %ZIP%
if exist "%ZIP%" del /q "%ZIP%"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Compress-Archive -Path '%DIST%' -DestinationPath '%ZIP%' -Force"
if errorlevel 1 (
    echo [오류] zip 생성에 실패했습니다. %DIST% 폴더는 그대로 사용할 수 있습니다.
    exit /b 1
)

echo.
echo 완료: %NAME% %VER%
echo   %DIST%
echo   %ZIP%
endlocal
exit /b 0
