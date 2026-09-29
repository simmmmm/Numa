#[repr(C)]
#[derive(Debug)]
struct MachTimebaseInfo {
    numerator: u32,
    denominator: u32,
}
extern "C" {
    fn mach_timebase_info(out: *mut MachTimebaseInfo) -> u32;
    fn mach_absolute_time() -> u64;
}

#[derive(Debug)]
pub struct PresentationTimer {
    scale: MachTimebaseInfo,
}
impl PresentationTimer {

    pub fn new() -> Self {

        let mut scale = MachTimebaseInfo {
            numerator: 1,
            denominator: 1,
        };
        unsafe { mach_timebase_info(&mut scale) };

        Self { scale }
    }

    pub fn get_timestamp_ns(&self) -> u128 {
        let time = unsafe { mach_absolute_time() };

        (time as u128 * self.scale.numerator as u128) / self.scale.denominator as u128
    }
}
