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
use std::fs::{File, OpenOptions};
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
    pub fn current_vt(&self) -> io::Result<Vt>{
        let vtstate = ffi::vt_getstate(self.file.as_raw_fd())?;
        Ok(Vt::with_number(self, vtstate.v_active, false))
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
    pub fn new_vt_with_minimum_number(&self, min: u16) -> io::Result<Vt> {
        
        // Get the first available vt number
        let mut n = ffi::vt_openqry(self.file.as_raw_fd())? as u16;
        let mut vt: Vt;

        if n >= min {
            vt = Vt::with_number(self, n, true);
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
                vt = Vt::with_number(self, n, true);
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
                    first_free = ffi::vt_openqry(self.file.as_raw_fd())? as u16;
                    files.push(OpenOptions::new().read(true).write(true).open(format!("/dev/tty{}", first_free))?);
                }

                n = first_free;
                vt = Vt::with_number_and_file(self, n, files.pop().unwrap(), true)?;

            }
        }

        // Make sure that the vt is open
        vt.ensure_open()?;

        // By default we turn off echo and signal generation.
        // We also disable Ctrl+D for EOF, since we will almost never want it.
        let termios = vt.termios.as_mut().unwrap();
        termios.input_flags |= InputFlags::IGNBRK;
        termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ISIG);
        termios.control_chars[SpecialCharacterIndices::VEOF as usize] = 0;
        vt.update_termios()?;

        Ok(vt)
    }

    /// Switches to the virtual terminal with the given number.
    pub fn switch_to(&self, vt_number: u16) -> io::Result<()> {
        ffi::vt_activate(self.file.as_raw_fd(), c_int::from(vt_number))?;
        ffi::vt_waitactive(self.file.as_raw_fd(), c_int::from(vt_number))
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

/// An allocated virtual terminal.
pub struct Vt<'a> {
    console: &'a Console,
    number: u16,
    file: Option<File>,
    termios: Option<Termios>,
    is_owned: bool
}

impl<'a> Vt<'a> {
    
    fn with_number(console: &'a Console, number: u16, owned: bool) -> Vt<'a> {
        Vt {
            console,
            number,
            file: None,
            termios: None,
            is_owned: owned
        }
    }

    fn with_number_and_file(console: &'a Console, number: u16, file: File, owned: bool) -> io::Result<Vt<'a>> {
        let mut vt = Vt {
            console,
            number,
            file: Some(file),
            termios: None,
            is_owned: owned
        };
        vt.ensure_open()?;
        Ok(vt)
    }

    fn ensure_open(&mut self) -> io::Result<()> {

        if self.file.is_none() {
            // Open the device corresponding to the number of this vt
            let path = format!("/dev/tty{}", self.number);
            self.file = Some(OpenOptions::new().read(true).write(true).open(path)?);
        }

        if self.termios.is_none() {
            // Get the termios info for the current vt
            let termios = tcgetattr(self.file.as_ref().unwrap().as_raw_fd())
                          .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))?;
            self.termios = Some(termios);
        }

        Ok(())
    }

    fn update_termios(&self) -> io::Result<()> {
        tcsetattr(
            self.file.as_ref().unwrap().as_raw_fd(),
            SetArg::TCSANOW,
            self.termios.as_ref().unwrap()
        )
        .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))
    }

    /// Returns the number of this virtual terminal.
    pub fn number(&self) -> u16 {
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
        self.flush()?;
        Ok(self)
    }

    /// Sets the blank timer for this terminal. A value of `0` disables the timer.
    /// 
    /// Returns `self` for chaining.
    pub fn set_blank_timer(&mut self, timer: u32) -> io::Result<&mut Self> {
        write!(self, "\x1b[9;{}]", timer)?;
        self.flush()?;
        Ok(self)
    }

    /// Blanks or unlanks the terminal.
    /// 
    /// Returns `self` for chaining.
    pub fn blank(&mut self, blank: bool) -> io::Result<&mut Self> {
        self.ensure_open()?;
        
        // If the console blanking timer is disabled, the ioctl below will fail,
        // so we need to enable it just for the time needed for the ioctl to work.
        let needs_timer_reset = if blank && self.console.blank_timer()? == 0 {
            self.set_blank_timer(1)?;
            true
        } else {
            false
        };

        let mut arg = if blank { ffi::TIOCL_BLANKSCREEN } else { ffi::TIOCL_UNBLANKSCREEN };
        ffi::tioclinux(self.file.as_ref().unwrap().as_raw_fd(), &mut arg)?;

        // Disable the blank timer if originally it was disabled
        if needs_timer_reset {
            self.set_blank_timer(0)?;
        }

        Ok(self)
    }

    /// Enables or disables the echo of the characters typed by the user.
    /// 
    /// Returns `self` for chaining.
    pub fn echo(&mut self, echo: bool) -> io::Result<&mut Self> {
        self.ensure_open()?;

        if echo {
            self.termios.as_mut().unwrap().local_flags |= LocalFlags::ECHO;
        } else {
            self.termios.as_mut().unwrap().local_flags &= !LocalFlags::ECHO;
        }
        self.update_termios()?;

        Ok(self)
    }

    /// Enables or disables signal generation from terminal.
    /// 
    /// Returns `self` for chaining.
    pub fn signals(&mut self, signals: VtSignals) -> io::Result<&mut Self> {
        self.ensure_open()?;
        
        // Since we created the vt with signals disabled, we need to enable them
        let termios = self.termios.as_mut().unwrap();
        termios.local_flags |= LocalFlags::ISIG;

        // Now we enable/disable the single signals
        if signals.contains(VtSignals::SIGINT) {
            termios.control_chars[SpecialCharacterIndices::VINTR as usize] = 0;
        } else {
            termios.control_chars[SpecialCharacterIndices::VINTR as usize] = 3;
        }
        if signals.contains(VtSignals::SIGQUIT) {
            termios.control_chars[SpecialCharacterIndices::VQUIT as usize] = 0;
        } else {
            termios.control_chars[SpecialCharacterIndices::VQUIT as usize] = 34;
        }
        if signals.contains(VtSignals::SIGTSTP) {
            termios.control_chars[SpecialCharacterIndices::VSUSP as usize] = 0;
        } else {
            termios.control_chars[SpecialCharacterIndices::VSUSP as usize] = 32;
        }
        self.update_termios()?;

        Ok(self)
    }

}

impl<'a> Drop for Vt<'a> {
    fn drop(&mut self) {
        if self.is_owned {
            // Notify the kernel that we do not need the vt anymore.
            // Note we don't check the return value because we have no way to recover from a closing error.
            let _ = ffi::vt_disallocate(self.console.file.as_raw_fd(), c_int::from(self.number));
        }
    }
}

/// Reading from a [`Vt`] reads directly from the underlying terminal.
/// 
/// [`Vt`]: crate::Vt
impl<'a> Read for Vt<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.ensure_open()?;
        self.file.as_ref().unwrap().read(buf)
    }
}

/// Writing to a [`Vt`] writes directly to the underlying terminal.
/// 
/// [`Vt`]: crate::Vt
impl<'a> Write for Vt<'a> {

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.ensure_open()?;
        self.file.as_ref().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.ensure_open()?;
        self.file.as_ref().unwrap().flush()?;
        tcflush(self.file.as_ref().unwrap().as_raw_fd(), FlushArg::TCIFLUSH)
            .map_err(|e| io::Error::from_raw_os_error(e.as_errno().unwrap_or(nix::errno::Errno::UnknownErrno) as i32))
    }

}