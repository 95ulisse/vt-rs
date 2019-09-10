use std::io::{self, Read};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use crate::ffi;
use crate::vt::{Vt, VtNumber, AsVtNumber};

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
        let vt: Vt;

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

        Ok(vt)
    }

    /// Releases the kernel resources for the terminal with the given number.
    pub(crate) fn disallocate_vt<N:AsVtNumber>(&self, vt_number: N) -> io::Result<()> {
        ffi::vt_disallocate(self.file.as_raw_fd(), vt_number.as_vt_number().as_native())
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