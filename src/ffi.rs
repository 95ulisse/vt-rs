use std::io;
use std::os::unix::io::RawFd;
use nix::libc::*;

// Some constants missing from `libc`
pub const VT_OPENQRY: c_int          = 0x5600;
pub const VT_GETSTATE: c_int         = 0x5603;
pub const VT_ACTIVATE: c_int         = 0x5606;
pub const VT_WAITACTIVE: c_int       = 0x5607;
pub const VT_DISALLOCATE: c_int      = 0x5608;
pub const VT_LOCKSWITCH: c_int       = 0x560B;
pub const VT_UNLOCKSWITCH: c_int     = 0x560C;
pub const TIOCL_BLANKSCREEN: c_int   = 14;
pub const TIOCL_UNBLANKSCREEN: c_int = 4;

// Structures for the vt ioctls
#[repr(C)]
pub struct VtStat {
	pub v_active: c_ushort,
	pub v_signal: c_ushort,
	pub v_state: c_ushort
}

macro_rules! ioctl_get_wrapper {
    ($fname:ident, $code:ident, $t:ty) => {
        #[inline]
        pub fn $fname(fd: RawFd) -> io::Result<$t> {
            unsafe {
                let mut data: $t = ::std::mem::uninitialized();
                let res = loop {
                    let res = ioctl(fd, $code as _, &mut data);
                    if res != EINTR {
                        break res;
                    }
                };
                match res {
                    -1 => Err(io::Error::from_raw_os_error(res)),
                    _ => Ok(data)
                }
            }
        }
    };
}

macro_rules! ioctl_set_wrapper {
    ($fname:ident, $code:ident, $t:ty) => {
        #[inline]
        pub fn $fname(fd: RawFd, arg: $t) -> io::Result<()> {
            unsafe {
                let res = loop {
                    let res = ioctl(fd, $code as _, arg);
                    if res != EINTR {
                        break res;
                    }
                };
                match res {
                    -1 => Err(io::Error::from_raw_os_error(res)),
                    _ => Ok(())
                }
            }
        }
    };
}

// Ioctl function wrappers
ioctl_get_wrapper!(vt_openqry, VT_OPENQRY, c_int);
ioctl_get_wrapper!(vt_getstate, VT_GETSTATE, VtStat);
ioctl_set_wrapper!(vt_activate, VT_ACTIVATE, c_int);
ioctl_set_wrapper!(vt_waitactive, VT_WAITACTIVE, c_int);
ioctl_set_wrapper!(vt_disallocate, VT_DISALLOCATE, c_int);
ioctl_set_wrapper!(vt_lockswitch, VT_LOCKSWITCH, c_int);
ioctl_set_wrapper!(vt_unlockswitch, VT_UNLOCKSWITCH, c_int);
ioctl_set_wrapper!(tioclinux, TIOCLINUX, *mut c_int);