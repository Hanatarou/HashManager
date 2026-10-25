/// Processors of this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuInfo {
    /// Logical processors (hardware threads).
    pub logical: usize,
    /// Physical cores.
    pub physical: usize,
}

impl CpuInfo {
    pub fn detect() -> CpuInfo {
        let logical = num_cpus::get().max(1);
        let physical = num_cpus::get_physical().clamp(1, logical);
        CpuInfo { logical, physical }
    }

    /// Hyper-Threading (SMT) is on when there are more logical processors than cores.
    pub fn smt(&self) -> bool {
        self.logical > self.physical
    }
}
