@echo off
if not exist "shaders-cache" mkdir "shaders-cache"
"C:\VulkanSDK\1.3.236.0\Bin\glslc.exe" shaders\shader.vert -o shaders-cache\vert.spv
"C:\VulkanSDK\1.3.236.0\Bin\glslc.exe" shaders\shader.frag -o shaders-cache\frag.spv