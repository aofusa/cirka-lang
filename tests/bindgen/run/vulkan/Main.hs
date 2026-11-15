-- The module Vulkan is written by `achc bindgen vulkan.h` (vulkan_core.h)
-- and the program is linked with the Vulkan loader (`--link=vulkan`).
import Foreign
import Foreign.C
import Vulkan

main :: IO ()
main = do
  -- Constants from function-like macros and enums.
  print (c_VK_API_VERSION_1_0, c_VK_API_VERSION_1_1)
  print (c_VK_STRUCTURE_TYPE_APPLICATION_INFO, c_VK_SUCCESS)
  print c_VK_MAX_PHYSICAL_DEVICE_NAME_SIZE
  -- Layouts as clang computes them.
  print (sizeOf (undefined :: VkApplicationInfo), sizeOf (undefined :: VkInstanceCreateInfo))
  print (sizeOf (undefined :: VkPhysicalDeviceProperties), sizeOf (undefined :: VkPhysicalDeviceLimits))
  -- The loader answers without a driver.
  alloca $ \p -> do
    r <- c_vkEnumerateInstanceVersion p
    v <- peek p
    print (r == c_VK_SUCCESS, v `shiftR` 22 >= 1)
  -- A struct written to C memory and read back.
  withCString "bindgen" $ \name -> do
    let app = VkApplicationInfo c_VK_STRUCTURE_TYPE_APPLICATION_INFO nullPtr name 1 nullPtr 0 c_VK_API_VERSION_1_0
    with app $ \p -> do
      app' <- peek p
      print (app' == app, vkApplicationInfo_apiVersion app')
