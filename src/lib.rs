//! # vt-rs
//! 
//! Rust bindings for the Linux virtual terminal APIs.
//! 
//! ```rust,no_run
//! # use std::io::Write;
//! use vt::Console;
//! 
//! // First of all, get a handle to the console
//! let console = Console::open().unwrap();
//! 
//! // Allocate a new virtual terminal
//! let mut vt = console.new_vt().unwrap();
//! 
//! // Write something to it.
//! // A `Vt` structure implements both `std::io::Read` and `std::io::Write`.
//! writeln!(vt, "Hello world!");
//! 
//! // Switch to the newly allocated terminal
//! vt.switch().unwrap();
//! ```
//! 
//! For a more complete example, see the files in the `examples` folder.

#[macro_use] extern crate bitflags;

use std::io::{self, Write, Read};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use nix::libc::*;
use nix::sys::termios::{
    Termios, InputFlags, LocalFlags, FlushArg, SetArg, SpecialCharacterIndices,
    tcgetattr, tcsetattr, tcflush
};

mod ffi;

/// Handle to a console device file, usually located at `/dev/console`.
/// This structure allows managing virtual terminals.
pub struct Console {
    file: File
}

impl Console {

    /// Opens a new handle to the console device file.    
    pub fn open() -> Result<Console, io::Error> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/console")
            .map(|file| Console { file })
    }

    /// Returns the currently active virtual terminal.
    pub fn current_vt_number(&self) -> io::Result<VtNumber>{
        let vtstate = ffi::vt_getstate(self.file.as_raw_fd())?;
        Ok(VtNumber::new(vtstate.v_active.into()))
    }

    /// Allocates a new virtual terminal.
    /// To switch to the newly created terminal, use [`Vt::switch`] or [`Console::switch_to`].
    /// 
    /// [`Console::switch_to`]: crate::Console::switch_to
    /// [`Vt::switch`]: crate::Vt::switch
    pub fn new_vt(&self) -> io::Result<Vt> {
        self.new_vt_with_minimum_number(0)
    }

    /// Allocates a new virtual terminal with a number greater than or equal to the given number.
    /// Be careful not to exaggerate too much with the minimum threshold: usually systems have
    /// a maximum number of 16 or 64 vts.
    /// 
    /// To switch to the newly created terminal, use [`Vt::switch`] or [`Console::switch_to`].
    /// 
    /// [`Console::switch_to`]: crate::Console::switch_to
    /// [`Vt::switch`]: crate::Vt::switch
    pub fn new_vt_with_minimum_number(&self, min: i32) -> io::Result<Vt> {
        
        // Get the first available vt number
        let mut n = ffi::vt_openqry(self.file.as_raw_fd())? as i32;
        let mut vt: Vt;

        if n >= min {
            vt = Vt::with_number(self, n.into())?;
        } else {
            n = min;

            // Fast path: the kernel provides a quick way to get the state of the first 16 vts
            // by returning a mask with 1s indicating the ones in use.
            let vtstate = ffi::vt_getstate(self.file.as_raw_fd())?;
            let mut found = false;
            let mut mask = 1 << n;
            while n < 16 {
                if vtstate.v_state & mask == 0 {
                    found = true;
                    break;
                }
                n += 1;
                mask <<= 1;
            }

            if found {
                vt = Vt::with_number(self, n.into())?;
            } else {

                // Slow path: we might be unlucky, and all the first 16 vts are already occupied.
                // This should never happen in a real case, but better safe than sorry.
                //
                // Here the kernel does not help us and we have to test each single vt one by one:
                // by issuing a VT_OPENQRY ioctl we can get back the first free vt.
                // We keep opening file descriptors until the next free vt is greater than `min`.
                //
                // I don't have words to describe how ugly and problematic this is,
                // but it's the only stable working solution I found. I seriously hope that this will never be needed.
                
                let mut files: Vec<File> = Vec::new();
                
                let mut first_free = 0;
                while first_free < n {
                    first_free = ffi::vt_openqry(self.file.as_raw_fd())? as i32;
                    files.push(OpenOptions::new().read(true).write(true).open(format!("/dev/tty{}", first_free))?);
                }

                n = first_free;
                vt = Vt::with_number_and_file(self, n.into(), files.pop().unwrap())?;

            }
        }

        // By default we turn off echo and signal generation.
        // We also disable Ctrl+D for EOF, since we will almost never want it.
        vt.termios.input_flags |= InputFlags::IGNBRK;
        vt.termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ISIG);
        vt.termios.control_chars[SpecialCharacterIndices::VEOF as usize] = 0;
        vt.update_termios()?;

        Ok(vt)
    }

    /// Opens the terminal with the given number.
    pub fn open_vt<N: AsVtNumber>(&self, vt_number: N) -> io::Result<Vt> {
        Vt::with_number(self, vt_number.as_vt_number())
    }

    /// Switches to the virtual terminal with the given number.
    pub fn switch_to<N: AsVtNumber>(&self, vt_number: N) -> io::Result<()> {
        let n = vt_number.as_vt_number().as_native();
        ffi::vt_activate(self.file.as_raw_fd(), n)?;
        ffi::vt_waitactive(self.file.as_raw_fd(), n)
    }

    /// Enables or disables virtual terminal switching (usually done with `Ctrl + Alt + F<n>`).
    pub fn lock_switch(&self, lock: bool) -> io::Result<()> {
        if lock {
            ffi::vt_lockswitch(self.file.as_raw_fd(), 1)
        } else {
            ffi::vt_unlockswitch(self.file.as_raw_fd(), 1)
        }
    }

    /// Returns the current console blank timer value. A value of `0` means that the timer is disabled.
    /// To change the blank timer, use the [`Vt::set_blank_timer`] method.
    /// 
    /// [`Vt::set_blank_timer`]: crate::Vt::set_blank_timer
    pub fn blank_timer(&self) -> io::Result<u32> {
        OpenOptions::new().read(true).open("/sys/module/kernel/parameters/consoleblank")
            .and_then(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).map(|_| s.trim().parse().expect("Expected consoleblank to contain an unsigned integer"))
            })
    }

}

/// A trait to extract the raw terminal number from an object.
pub trait AsVtNumber {

    /// Returns the underlying terminal number of this object.
    fn as_vt_number(&self) -> VtNumber;
    
}

/// Number of a virtual terminal.
///
/// Can be opened to get full access to the terminal.
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

    fn as_native(self) -> c_int {
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
    
    fn with_number(console: &'a Console, number: VtNumber) -> io::Result<Vt<'a>> {
        
        // Open the device corresponding to the number of this vt
        let path = format!("/dev/tty{}", number);
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        Vt::with_number_and_file(console, number, file)
    }

    fn with_number_and_file(console: &'a Console, number: VtNumber, file: File) -> io::Result<Vt<'a>> {
        
        // Get the termios info for the current file
        let termios = tcgetattr(file.as_raw_fd())
                      .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))?;

        Ok(Vt {
            console,
            number,
            file,
            termios
        })
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

}

impl<'a> Drop for Vt<'a> {
    fn drop(&mut self) {
        // Notify the kernel that we do not need the vt anymore.
        // Note we don't check the return value because we have no way to recover from a closing error.
        let _ = ffi::vt_disallocate(self.console.file.as_raw_fd(), self.number.as_native());
    }
}

impl<'a> AsVtNumber for Vt<'a> {
    fn as_vt_number(&self) -> VtNumber {
        self.number
    }
}

impl<'a> Deref for Vt<'a> {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl<'a> DerefMut for Vt<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }   
}