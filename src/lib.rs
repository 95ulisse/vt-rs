use std::io::{self, Write, Read};
use std::fs::{File, OpenOptions};
use nix::sys::termios::Termios;

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
        
    }

    pub fn new_vt(&self) -> io::Result<Vt> {

    }

    pub fn new_vt_with_minimum_number(&self, min: u32) -> io::Result<Vt> {

    }

    pub fn switch_to(&self, vt_number: u32) -> io::Result<()> {

    }

    pub fn lock_switch(&self, lock: bool) -> io::Result<()> {

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
    number: u32,
    file: Option<File>,
    termios: Option<Termios>
}

impl<'a> Vt<'a> {
    
    fn with_number(console: &'a Console, number: u32) -> Vt<'a> {
        Vt {
            console,
            number,
            file: None,
            termios: None
        }
    }

    pub fn switch(&self) -> io::Result<()> {

    }

    pub fn clear(&self) -> io::Result<()> {

    }

    pub fn blank(&self, blank: bool) -> io::Result<()> {

    }

    pub fn echo(&self, echo: bool) -> io::Result<()> {

    }

    pub fn signals(&self, signals: VtSignals) -> io::Result<()> {

    }

    pub fn mode(&self, mode: VtMode) -> io::Result<()> {
        
    }

}

impl<'a> Read for Vt<'a> {

}

impl<'a> Write for Vt<'a> {

}