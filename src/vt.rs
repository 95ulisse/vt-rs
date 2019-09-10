use std::io::{self, Write, Read, IoSlice, IoSliceMut};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::os::unix::io::{RawFd, AsRawFd};
use nix::libc::*;
use nix::sys::termios::{
    Termios, InputFlags, LocalFlags, FlushArg, SetArg, SpecialCharacterIndices,
    tcgetattr, tcsetattr, tcflush, cfmakeraw
};
use crate::ffi;
use crate::console::Console;

/// A trait to extract the raw terminal number from an object.
pub trait AsVtNumber {

    /// Returns the underlying terminal number of this object.
    fn as_vt_number(&self) -> VtNumber;

}

/// Number of a virtual terminal.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VtNumber(i32);

impl VtNumber {

    /// Creates a new `VtNumber` for the given integer.
    /// Panics if the number is negative.
    pub fn new(number: i32) -> VtNumber {
        if number < 0 {
            panic!("Invalid virtual terminal number.");
        }
        VtNumber(number)
    }

    pub(crate) fn as_native(self) -> c_int {
        self.0
    }

}

impl From<i32> for VtNumber {
    fn from(number: i32) -> VtNumber {
        VtNumber::new(number)
    }
}

impl AsVtNumber for VtNumber {
    fn as_vt_number(&self) -> VtNumber {
        *self
    }
}

impl fmt::Display for VtNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

bitflags! {
    /// Enum containing all the signals supported by the virtual terminal.
    /// Use [`Vt::signals`] to manage the signals enabled in a virtual terminal.
    /// 
    /// [`Vt::signals`]: crate::Vt::signals
    pub struct VtSignals: u8 {
        const SIGINT  = 1;
        const SIGQUIT = 1 << 1;
        const SIGTSTP = 1 << 2;
    }
}

/// Enum containing the VT buffers to flush.
pub enum VtFlushType {
    Incoming,
    Outgoing,
    Both
}

/// An allocated virtual terminal.
pub struct Vt<'a> {
    console: &'a Console,
    number: VtNumber,
    file: File,
    termios: Termios
}

impl<'a> Vt<'a> {
    
    pub(crate) fn with_number(console: &'a Console, number: VtNumber) -> io::Result<Vt<'a>> {
        
        // Open the device corresponding to the number of this vt
        let path = format!("/dev/tty{}", number);
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        Vt::with_number_and_file(console, number, file)
    }

    pub(crate) fn with_number_and_file(console: &'a Console, number: VtNumber, file: File) -> io::Result<Vt<'a>> {
        
        // Get the termios info for the current file
        let mut termios = tcgetattr(file.as_raw_fd())
                          .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))?;

        // By default we turn off echo and signal generation.
        // We also disable Ctrl+D for EOF, since we will almost never want it.
        termios.input_flags |= InputFlags::IGNBRK;
        termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ISIG);
        termios.control_chars[SpecialCharacterIndices::VEOF as usize] = 0;

        let vt = Vt {
            console,
            number,
            file,
            termios
        };

        vt.update_termios()?;

        Ok(vt)
    }

    fn update_termios(&self) -> io::Result<()> {
        tcsetattr(
            self.file.as_raw_fd(),
            SetArg::TCSANOW,
            &self.termios
        )
        .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))
    }

    /// Returns the number of this virtual terminal.
    pub fn number(&self) -> VtNumber {
        self.number
    }

    /// Switches to this virtual terminal. This is just a shortcut for [`Console::switch_to`].
    /// 
    /// Returns `self` for chaining.
    /// 
    /// [`Console::switch_to`]: crate::Console::switch_to
    pub fn switch(&self) -> io::Result<&Self> {
        self.console.switch_to(self.number)?;
        Ok(self)
    }

    /// Clears the terminal.
    /// 
    /// Returns `self` for chaining.
    pub fn clear(&mut self) -> io::Result<&mut Self> {
        write!(self, "\x1b[H\x1b[J")?;
        Ok(self)
    }

    /// Sets the blank timer for this terminal. A value of `0` disables the timer.
    /// 
    /// Returns `self` for chaining.
    pub fn set_blank_timer(&mut self, timer: u32) -> io::Result<&mut Self> {
        write!(self, "\x1b[9;{}]", timer)?;
        Ok(self)
    }

    /// Blanks or unlanks the terminal.
    /// 
    /// Returns `self` for chaining.
    pub fn blank(&mut self, blank: bool) -> io::Result<&mut Self> {
        
        // If the console blanking timer is disabled, the ioctl below will fail,
        // so we need to enable it just for the time needed for the ioctl to work.
        let needs_timer_reset = if blank && self.console.blank_timer()? == 0 {
            self.set_blank_timer(1)?;
            true
        } else {
            false
        };

        let mut arg = if blank { ffi::TIOCL_BLANKSCREEN } else { ffi::TIOCL_UNBLANKSCREEN };
        ffi::tioclinux(self.file.as_raw_fd(), &mut arg)?;

        // Disable the blank timer if originally it was disabled
        if needs_timer_reset {
            self.set_blank_timer(0)?;
        }

        Ok(self)
    }

    /// Enables or disables the echo of the characters typed by the user.
    /// 
    /// Returns `self` for chaining.
    pub fn set_echo(&mut self, echo: bool) -> io::Result<&mut Self> {
        if echo {
            self.termios.local_flags |= LocalFlags::ECHO;
        } else {
            self.termios.local_flags &= !LocalFlags::ECHO;
        }
        self.update_termios()?;

        Ok(self)
    }

    /// Returns a value indicating whether this terminal has echo enabled or not.
    pub fn is_echo_enabled(&self) -> bool {
        self.termios.local_flags.contains(LocalFlags::ECHO)
    }

    /// Enables or disables signal generation from terminal.
    /// 
    /// Returns `self` for chaining.
    pub fn signals(&mut self, signals: VtSignals) -> io::Result<&mut Self> {
        
        // Since we created the vt with signals disabled, we need to enable them
        self.termios.local_flags |= LocalFlags::ISIG;

        // Now we enable/disable the single signals
        if !signals.contains(VtSignals::SIGINT) {
            self.termios.control_chars[SpecialCharacterIndices::VINTR as usize] = 0;
        } else {
            self.termios.control_chars[SpecialCharacterIndices::VINTR as usize] = 3;
        }
        if !signals.contains(VtSignals::SIGQUIT) {
            self.termios.control_chars[SpecialCharacterIndices::VQUIT as usize] = 0;
        } else {
            self.termios.control_chars[SpecialCharacterIndices::VQUIT as usize] = 34;
        }
        if !signals.contains(VtSignals::SIGTSTP) {
            self.termios.control_chars[SpecialCharacterIndices::VSUSP as usize] = 0;
        } else {
            self.termios.control_chars[SpecialCharacterIndices::VSUSP as usize] = 32;
        }
        self.update_termios()?;

        Ok(self)
    }

    /// Flushes the internal buffers of the terminal.
    pub fn flush_buffers(&mut self, t: VtFlushType) -> io::Result<&mut Self> {
        let action = match t {
            VtFlushType::Incoming => FlushArg::TCIFLUSH,
            VtFlushType::Outgoing => FlushArg::TCOFLUSH,
            VtFlushType::Both => FlushArg::TCIOFLUSH
        };
        tcflush(self.file.as_raw_fd(), action)
            .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))?;

        Ok(self)
    }

    /// Configures the terminal in raw mode: input is available character by character,
    /// echoing is disabled, and all special processing of terminal input and output characters is disabled.
    pub fn raw(&mut self) -> io::Result<&mut Self> {
        cfmakeraw(&mut self.termios);
        self.update_termios()?;
        Ok(self)
    }

}

impl<'a> Drop for Vt<'a> {
    fn drop(&mut self) {
        // Notify the kernel that we do not need the vt anymore.
        // Note we don't check the return value because we have no way to recover from a closing error.
        let _ = self.console.disallocate_vt(self.number);
    }
}

impl<'a> AsVtNumber for Vt<'a> {
    fn as_vt_number(&self) -> VtNumber {
        self.number
    }
}

impl<'a> AsRawFd for Vt<'a> {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

/// Delegates the implementation of [`Read`] to the underlying [`File`].
/// 
/// [`Read`]: std::io::Read
/// [`File`]: std::fs::File
impl<'a> Read for Vt<'a> {

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut]) -> io::Result<usize> {
        self.file.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.file.read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.file.read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.file.read_exact(buf)
    }

}

/// Delegates the implementation of [`Write`] to the underlying [`File`].
/// 
/// [`Write`]: std::io::Write
/// [`File`]: std::fs::File
impl<'a> Write for Vt<'a> {

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }

    fn write_vectored(&mut self, bufs: &[IoSlice]) -> io::Result<usize> {
        self.file.write_vectored(bufs)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.file.write_all(buf)
    }

    fn write_fmt(&mut self, fmt: fmt::Arguments) -> io::Result<()> {
        self.file.write_fmt(fmt)
    }

}