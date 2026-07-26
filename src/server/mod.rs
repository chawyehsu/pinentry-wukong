pub mod command;
pub mod pinentry;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::start;
#[cfg(windows)]
pub use windows::start;
