fn main() {
    cfg_aliases::cfg_aliases! {
        native: { not(target_arch = "wasm32") },
        send_sync: { any(
            not(target_arch = "wasm32"),
            all(feature = "fragile-send-sync-non-atomic-wasm", not(target_feature = "atomics"))
        ) },
        webgl: { all(target_arch = "wasm32", not(target_os = "emscripten"), gles) },
        Emscripten: { all(target_os = "emscripten", gles) },
        dx12: { all(target_os = "windows", feature = "dx12") },
        gles: { all(feature = "gles") },

        gles_with_std: { all(
            feature = "gles",
            any(
                not(target_arch = "wasm32"),

                all(target_vendor = "unknown", target_os = "unknown"),

                target_os = "emscripten"
            )
        ) },
        metal: { all(target_vendor = "apple", feature = "metal") },
        vulkan: { all(not(target_arch = "wasm32"), feature = "vulkan") },
        drm: { all(
            feature = "drm",
            any(target_os = "linux", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd")
        ) },
        any_backend: { any(dx12, metal, vulkan, gles) },

        static_dxc: { all(target_os = "windows", feature = "static-dxc", not(target_arch = "aarch64"), target_env = "msvc") },
        supports_64bit_atomics: { target_has_atomic = "64" },
        supports_ptr_atomics: { target_has_atomic = "ptr" }
    }
}
