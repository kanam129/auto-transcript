@echo off
rem Builds the Windows application with GPU acceleration compiled in.
rem
rem The resulting binary decides at runtime: it uses the GPU when the machine has a
rem Vulkan device and falls back to the CPU when it does not, so this one build serves
rem both. See src-tauri/src/stt/engine.rs.
rem
rem Ninja is used instead of the default MSBuild generator on purpose. ggml builds its
rem Vulkan shader generator in a deeply nested sub-project and MSBuild's .tlog files push
rem the path past the 260-character Windows limit, which fails with an error that does not
rem mention paths at all.
setlocal

if not defined VULKAN_SDK (
  for /d %%d in ("C:\VulkanSDK\*") do set "VULKAN_SDK=%%d"
)
if not defined VULKAN_SDK (
  echo Vulkan SDK not found. Install it from https://vulkan.lunarg.com/sdk/home
  echo or build without GPU support using: npm run tauri build
  exit /b 1
)
echo Vulkan SDK: %VULKAN_SDK%

set "VS="
for %%e in (BuildTools Community Professional Enterprise) do (
  if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\%%e\VC\Auxiliary\Build\vcvars64.bat" (
    if not defined VS set "VS=C:\Program Files (x86)\Microsoft Visual Studio\2022\%%e"
  )
)
if not defined VS (
  echo Visual Studio Build Tools 2022 with the C++ workload was not found.
  exit /b 1
)
call "%VS%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1

set "CMAKE_GENERATOR=Ninja"
set "PATH=%PATH%;%VS%\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja"

npm run tauri build -- --features gpu-vulkan
