#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

#[cfg(target_os = "linux")]
mod linux {
    use core::ffi::c_void;
    use libc::{c_char, dlsym, RTLD_NEXT};
    use std::ffi::CStr;
    use std::sync::OnceLock;

    // ---- FFI type aliases (opaque) ----
    pub enum libinput_event_pointer {}
    pub type libinput_pointer_axis = u32;
    pub type libinput_pointer_axis_source = u32;

    // ---- libinput constants (must match libinput headers) ----
    pub const LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL: libinput_pointer_axis = 0;
    pub const LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL: libinput_pointer_axis = 1;

    pub const LIBINPUT_POINTER_AXIS_SOURCE_WHEEL: libinput_pointer_axis_source = 1;
    pub const LIBINPUT_POINTER_AXIS_SOURCE_FINGER: libinput_pointer_axis_source = 2;
    pub const LIBINPUT_POINTER_AXIS_SOURCE_CONTINUOUS: libinput_pointer_axis_source = 3;
    pub const LIBINPUT_POINTER_AXIS_SOURCE_WHEEL_TILT: libinput_pointer_axis_source = 4;

    // We'll resolve libinput_event_pointer_get_axis_source via dlsym at runtime as well
    // to avoid a hard link-time dependency on libinput.

    // ---- Real function pointers (resolved via dlsym(RTLD_NEXT, ...)) ----
    type AxisValueFn = unsafe extern "C" fn(
        event: *mut libinput_event_pointer,
        axis: libinput_pointer_axis,
    ) -> f64;

    type AxisDiscreteFn = unsafe extern "C" fn(
        event: *mut libinput_event_pointer,
        axis: libinput_pointer_axis,
    ) -> f64;

    type ScrollV120Fn = unsafe extern "C" fn(
        event: *mut libinput_event_pointer,
        axis: u32,
    ) -> f64;

    type AxisSourceFn = unsafe extern "C" fn(
        event: *mut libinput_event_pointer,
    ) -> libinput_pointer_axis_source;

    static REAL_AXIS_VALUE: OnceLock<AxisValueFn> = OnceLock::new();
    static REAL_AXIS_DISCRETE: OnceLock<AxisDiscreteFn> = OnceLock::new();
    static REAL_SCROLL_V120: OnceLock<ScrollV120Fn> = OnceLock::new();
    static REAL_AXIS_SOURCE: OnceLock<AxisSourceFn> = OnceLock::new();

    unsafe fn resolve_symbol<T>(name: &'static CStr) -> Option<T> {
        let sym = dlsym(RTLD_NEXT, name.as_ptr() as *const c_char) as *mut c_void;
        if sym.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*mut c_void, T>(sym))
        }
    }

    fn get_real_axis_value() -> AxisValueFn {
        unsafe {
            *REAL_AXIS_VALUE.get_or_init(|| {
                resolve_symbol::<AxisValueFn>(
                    CStr::from_bytes_with_nul_unchecked(
                        b"libinput_event_pointer_get_axis_value\0",
                    ),
                )
                .unwrap_or_else(|| {
                    // This is bad; we cannot proceed sensibly.
                    eprintln!(
                        "[libinput_scroll_hook] FATAL: failed to resolve libinput_event_pointer_get_axis_value"
                    );
                    // Return a stub to avoid UB; always returns 0.0
                    unsafe extern "C" fn stub(
                        _event: *mut libinput_event_pointer,
                        _axis: libinput_pointer_axis,
                    ) -> f64 {
                        0.0
                    }
                    stub
                })
            })
        }
    }

    fn get_real_axis_discrete() -> AxisDiscreteFn {
        unsafe {
            *REAL_AXIS_DISCRETE.get_or_init(|| {
                resolve_symbol::<AxisDiscreteFn>(
                    CStr::from_bytes_with_nul_unchecked(
                        b"libinput_event_pointer_get_axis_value_discrete\0",
                    ),
                )
                .unwrap_or_else(|| {
                    eprintln!(
                        "[libinput_scroll_hook] WARN: failed to resolve libinput_event_pointer_get_axis_value_discrete; using passthrough"
                    );
                    unsafe extern "C" fn stub(
                        _event: *mut libinput_event_pointer,
                        _axis: libinput_pointer_axis,
                    ) -> f64 {
                        0.0
                    }
                    stub
                })
            })
        }
    }

    fn get_real_scroll_v120() -> ScrollV120Fn {
        unsafe {
            *REAL_SCROLL_V120.get_or_init(|| {
                resolve_symbol::<ScrollV120Fn>(
                    CStr::from_bytes_with_nul_unchecked(
                        b"libinput_event_pointer_get_scroll_value_v120\0",
                    ),
                )
                .unwrap_or_else(|| {
                    eprintln!(
                        "[libinput_scroll_hook] WARN: failed to resolve libinput_event_pointer_get_scroll_value_v120; using passthrough"
                    );
                    unsafe extern "C" fn stub(
                        _event: *mut libinput_event_pointer,
                        _axis: u32,
                    ) -> f64 {
                        0.0
                    }
                    stub
                })
            })
        }
    }

    fn get_real_axis_source() -> AxisSourceFn {
        unsafe {
            *REAL_AXIS_SOURCE.get_or_init(|| {
                resolve_symbol::<AxisSourceFn>(
                    CStr::from_bytes_with_nul_unchecked(
                        b"libinput_event_pointer_get_axis_source\0",
                    ),
                )
                .unwrap_or_else(|| {
                    eprintln!(
                        "[libinput_scroll_hook] FATAL: failed to resolve libinput_event_pointer_get_axis_source"
                    );
                    unsafe extern "C" fn stub(
                        _event: *mut libinput_event_pointer,
                    ) -> libinput_pointer_axis_source {
                        // Default to WHEEL so we do not scale if resolution fails
                        LIBINPUT_POINTER_AXIS_SOURCE_WHEEL
                    }
                    stub
                })
            })
        }
    }

    // ---- Scaling configuration ----
    #[derive(Clone, Copy, Debug)]
    struct ScaleCfg {
        x: f64,
        y: f64,
        wheel: Option<f64>, // Optional wheel scaling (normally None -> no scaling)
        debug: bool,
    }

    static SCALE_CFG: OnceLock<ScaleCfg> = OnceLock::new();

    fn read_env_f64(key: &str) -> Option<f64> {
        std::env::var(key).ok().and_then(|s| s.parse::<f64>().ok())
    }

    fn get_cfg() -> ScaleCfg {
        *SCALE_CFG.get_or_init(|| {
            // Defaults
            let default = 1.0_f64;
            let x = read_env_f64("LIBINPUT_SCROLL_SCALE_X").unwrap_or_else(|| {
                read_env_f64("LIBINPUT_SCROLL_SCALE").unwrap_or(default)
            });
            let y = read_env_f64("LIBINPUT_SCROLL_SCALE_Y").unwrap_or_else(|| {
                read_env_f64("LIBINPUT_SCROLL_SCALE").unwrap_or(default)
            });
            // Optional wheel scaling (disabled by default)
            let wheel = read_env_f64("LIBINPUT_SCROLL_SCALE_WHEEL");
            let debug = std::env::var("LIBINPUT_SCROLL_DEBUG").ok().map_or(false, |v| v != "0");

            let clamp = |v: f64| v.clamp(0.05, 20.0);
            let cfg = ScaleCfg { x: clamp(x), y: clamp(y), wheel: wheel.map(clamp), debug };
            if cfg.debug {
                eprintln!(
                    "[libinput_scroll_hook] cfg: x={:.3}, y={:.3}, wheel={:?}",
                    cfg.x, cfg.y, cfg.wheel
                );
            }
            cfg
        })
    }

    #[inline]
    fn should_scale_source(src: libinput_pointer_axis_source) -> bool {
        src == LIBINPUT_POINTER_AXIS_SOURCE_FINGER || src == LIBINPUT_POINTER_AXIS_SOURCE_CONTINUOUS
    }

    // ---- Hooked symbols ----
    #[no_mangle]
    pub unsafe extern "C" fn libinput_event_pointer_get_axis_value(
        event: *mut libinput_event_pointer,
        axis: libinput_pointer_axis,
    ) -> f64 {
        let real = get_real_axis_value();
        let mut v = real(event, axis);
        if event.is_null() {
            return v;
        }
        let src = get_real_axis_source()(event);
        let cfg = get_cfg();
        if should_scale_source(src) {
            let before = v;
            if axis == LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL {
                v *= cfg.x;
            } else if axis == LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL {
                v *= cfg.y;
            }
            if cfg.debug {
                eprintln!(
                    "[libinput_scroll_hook] axis_value src={} axis={} {:.3} -> {:.3}",
                    src, axis, before, v
                );
            }
        } else if cfg.debug {
            eprintln!(
                "[libinput_scroll_hook] axis_value passthrough src={} axis={} v={:.3}",
                src, axis, v
            );
        }
        v
    }

    #[no_mangle]
    pub unsafe extern "C" fn libinput_event_pointer_get_axis_value_discrete(
        event: *mut libinput_event_pointer,
        axis: libinput_pointer_axis,
    ) -> f64 {
        // Discrete values (wheel steps) must remain unchanged.
        let real = get_real_axis_discrete();
        let v = real(event, axis);
        let cfg = get_cfg();
        if cfg.debug {
            eprintln!(
                "[libinput_scroll_hook] axis_value_discrete passthrough axis={} v={:.3}",
                axis, v
            );
        }
        v
    }

    #[no_mangle]
    pub unsafe extern "C" fn libinput_event_pointer_get_scroll_value_v120(
        event: *mut libinput_event_pointer,
        axis: u32,
    ) -> f64 {
        // v120 is only for wheel events; keep unchanged unless explicitly configured.
        let real = get_real_scroll_v120();
        let mut v = real(event, axis);
        let cfg = get_cfg();
        if let Some(wheel_scale) = cfg.wheel {
            let before = v;
            v *= wheel_scale;
            if cfg.debug {
                eprintln!(
                    "[libinput_scroll_hook] scroll_value_v120 scaled axis={} {:.3} -> {:.3}",
                    axis, before, v
                );
            }
            return v;
        }
        if cfg.debug {
            eprintln!(
                "[libinput_scroll_hook] scroll_value_v120 passthrough axis={} v={:.3}",
                axis, v
            );
        }
        v
    }
}

// For non-Linux builds, provide an empty library so cargo metadata/builds don't fail locally.
#[cfg(not(target_os = "linux"))]
mod non_linux {
    // Intentionally left blank.
}
