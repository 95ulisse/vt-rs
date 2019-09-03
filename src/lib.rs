use std::io::{self, Write, Read};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use nix::libc::*;
use nix::sys::termios::Termios;

mod ffi;

pub struct Console {
    file: File
}

impl Console {
    
    pub fn open() -> Result<Console, io::Error> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/console")
            .map(|file| Console { file })
    }

    pub fn get_current_vt(&self) -> io::Result<Vt>{
        let vtstate = ffi::vt_getstate(self.file.as_raw_fd())?;
        Ok(Vt::with_number(self, vtstate.v_active))
    }

    pub fn new_vt(&self) -> io::Result<Vt> {
        unimplemented!()
    }

    pub fn new_vt_with_minimum_number(&self, min: u16) -> io::Result<Vt> {
        unimplemented!()
    }

    pub fn switch_to(&self, vt_number: u16) -> io::Result<()> {
        ffi::vt_activate(self.file.as_raw_fd(), c_int::from(vt_number))?;
        ffi::vt_waitactive(self.file.as_raw_fd(), c_int::from(vt_number))
    }

    pub fn lock_switch(&self, lock: bool) -> io::Result<()> {
        unimplemented!()
    }

}

pub enum VtSignals {
    SigInt,
    SigQuit,
    SigTstp
}

pub enum VtMode {
    Text,
    Graphical
}

pub struct Vt<'a> {
    console: &'a Console,
    number: u16,
    file: Option<File>,
    termios: Option<Termios>
}

impl<'a> Vt<'a> {
    
    fn with_number(console: &'a Console, number: u16) -> Vt<'a> {
        Vt {
            console,
            number,
            file: None,
            termios: None
        }
    }

    pub fn number(&self) -> u16 {
        self.number
    }

    pub fn switch(&self) -> io::Result<()> {
        unimplemented!()
    }

    pub fn clear(&self) -> io::Result<()> {
        unimplemented!()
    }

    pub fn blank(&self, blank: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn echo(&self, echo: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn signals(&self, signals: VtSignals) -> io::Result<()> {
        unimplemented!()
    }

    pub fn mode(&self, mode: VtMode) -> io::Result<()> {
        unimplemented!()
    }

}

/*impl<'a> Read for Vt<'a> {

}

impl<'a> Write for Vt<'a> {

}*/