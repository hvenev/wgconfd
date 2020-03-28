// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::{fs, io, mem};

#[repr(transparent)]
pub struct Temp {
    path: PathBuf,
}

impl Drop for Temp {
    fn drop(&mut self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        if let Err(err) = fs::remove_file(&self.path) {
            eprintln!("<3>Failed to clean up temporary file: {}", err);
        }
    }
}

impl Temp {
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    pub fn leave(mut self) -> PathBuf {
        mem::replace(&mut self.path, PathBuf::new())
    }

    #[inline]
    pub fn rename_to(self, to: impl AsRef<Path>) -> io::Result<()> {
        fs::rename(self.leave(), to)
    }
}

pub struct Writer {
    inner: Temp,
    file: fs::File,
}

impl Writer {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let mut file = fs::OpenOptions::new();
        file.create_new(true);
        file.append(true);
        #[cfg(unix)]
        file.mode(0o0600);
        let file = file.open(&path)?;

        Ok(Self {
            inner: Temp { path },
            file,
        })
    }

    pub fn new_in(path: &Path) -> io::Result<Self> {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        loop {
            let i: u64 = rng.next_u64();
            let mut p: PathBuf = path.into();
            p.push(format!(".tmp.{:16x}", i));
            match Self::new(p) {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if e.kind() != io::ErrorKind::AlreadyExists {
                        return Err(e);
                    }
                }
            }
        }
    }

    #[inline]
    pub fn file(&mut self) -> &mut fs::File {
        &mut self.file
    }

    #[inline]
    pub fn sync_done(self) -> io::Result<Temp> {
        self.file.sync_data()?;
        Ok(self.done())
    }

    #[inline]
    pub fn done(self) -> Temp {
        self.inner
    }
}

pub fn update(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp = Writer::new_in(path.parent().unwrap())?;
    io::Write::write_all(tmp.file(), data)?;
    tmp.sync_done()?.rename_to(path)
}

#[inline]
pub fn load(path: &impl AsRef<Path>) -> io::Result<Vec<u8>> {
    _load(path.as_ref())
}

fn _load(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(&path)?;
    let mut data = Vec::new();
    io::Read::read_to_end(&mut file, &mut data)?;
    Ok(data)
}
