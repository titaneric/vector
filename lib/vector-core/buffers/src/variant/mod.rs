// NOTE unfortunately because we can't edit out a lifetime based on a feature
// flag we need two copies of `Variant` else the liftime being unused when
// 'disk-buffers' is flagged off will ding the build.
#[cfg(not(feature = "disk-buffer"))]
mod memory_only;
#[cfg(not(feature = "disk-buffer"))]
pub use memory_only::*;

#[cfg(feature = "disk-buffer")]
mod disk_and_memory;
#[cfg(feature = "disk-buffer")]
pub use disk_and_memory::*;
