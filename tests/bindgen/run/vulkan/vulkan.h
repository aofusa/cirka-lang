// bindgen: --allowlist=vkEnumerateInstanceVersion|VkInstanceCreateInfo|vkGetPhysicalDeviceProperties|VK_API_VERSION_1_*|VK_STRUCTURE_TYPE_APPLICATION_INFO|VK_MAX_PHYSICAL_DEVICE_NAME_SIZE --link=vulkan
// requires: vulkan/vulkan_core.h
// vulkan_core.h (the Vulkan headers of the Docker image) through `achc
// bindgen`. The loader answers vkEnumerateInstanceVersion without a driver.
#include <vulkan/vulkan_core.h>
