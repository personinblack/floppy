use regex::Regex;
use chrono::prelude::*;
use rocket::{Data, Outcome::*};
use rocket::request::{Request, FromFormValue};
use rocket::data::{FromData, Outcome, Transform, Transformed};
use rocket::response::NamedFile;
use rocket::http::{Status, RawStr};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::fs;
use std::path::PathBuf;
use std::env::var;
use std::ops::Sub;

#[derive(Debug, Fail)]
pub enum FileError {
    #[fail(display = "Sorry, no file for you. Might be expired.")]
    NotFound,

    #[fail(display = "Sorry, this file is expired and you can not reach it no
                     more.")]
    Expired,

    #[fail(display = "Internal Error: {}", err)]
    Internal{ err: String },

    #[fail(display = "{}", dsp)]
    Other{ dsp: String }
}

impl From<&'static str> for FileError {
    fn from(display: &'static str) -> FileError {
        FileError::Other{ dsp: display.to_string() }
    }
}

impl From<io::Error> for FileError {
    fn from(error: io::Error) -> FileError {
        FileError::Internal{ err: error.to_string() }
    }
}

impl From<&io::Error> for FileError {
    fn from(error: &io::Error) -> FileError {
        FileError::Internal{ err: error.to_string() }
    }
}

fn path_with_prefix(path: String) -> PathBuf {
    let mut path_pref = PathBuf::from(var("SAVE_DIR").unwrap_or(String::new()));
    path_pref.push(path);
    path_pref
}

pub struct ExpGuardian {
    time: chrono::DateTime<chrono::Utc>
}

impl ExpGuardian {
    pub fn check(&mut self, minutes: u64) -> Result<(), FileError> {
        if Utc::now().sub(self.time).num_minutes() >= minutes as i64 {
            self.time = Utc::now();
            for dir in path_with_prefix("".into()).read_dir()? {
                println!("checking {:?}", dir.as_ref()?);
                let result = File::at(dir?.path()).delete_over()?;
                println!("is deleted: {}", result);
            };
        }

        Ok(())
    }

    pub fn check_hour(&mut self) -> Result<(), FileError> {
        self.check(60)
    }
}

impl Default for ExpGuardian {
    fn default() -> Self {
        Self { time: Utc::now() }
    }
}

pub struct File {
    /// Path to the file on disk.
    path: PathBuf,
    /// Time of creation.
    time: Option<DateTime<Utc>>,
    data: Option<Vec<u8>>,
}

impl File {
    pub fn new(data: Vec<u8>) -> File {
        let path = path_with_prefix(File::hashed(&data));
        if path.exists() && path.read_dir().unwrap().next().is_some() {
            File::at(path)
        } else {
            if path.exists() {
                fs::remove_dir_all(&path).unwrap();
            }

            File {
                path,
                time: Some(Utc::now()),
                data: Some(data)
            }
        }
    }

    pub fn at(path: PathBuf) -> File {
        File {
            path,
            time: None,
            data: None
        }
    }

    pub fn save(&self) -> Result<String, FileError> {
        if self.path.exists() {
            return Ok(
                "Someone has already uploaded this file before. \
                No need to recreate it.\r".to_string() + &self.info()?);
        } else if self.data.is_some() &&
                  self.data.as_ref().unwrap().len() > 150000000 {

            return Ok("I don't accept fat files. file_size > 150m"
                      .to_string());
        }

        fs::create_dir_all(&self.path).map_err(FileError::from)?;

        let mut pfile = PathBuf::from(&self.path);
        pfile.push(PathBuf::from(&self.time()?.to_rfc3339()));
        match fs::File::create(pfile).map_err(FileError::from)?
                .write_all(&self.data.as_ref().unwrap()[..]) {

            Ok(_) => Ok(self.info()?),
            Err(e) => Err(FileError::from(e))
        }
    }

    pub fn delete(&self) -> Result<(), FileError> {
        fs::remove_dir_all(&self.path).map_err(FileError::from)
    }

    pub fn delete_over(&self) -> Result<bool, FileError> {
        if self.days()? == 0.0 {
            self.delete().map(|_| true)
        } else {
            Ok(false)
        }
    }

    pub fn named_file(&self) -> Result<NamedFile, FileError> {
        NamedFile::open(match fs::read_dir(&self.path)
                        .map_err(|_| FileError::NotFound)?.next() {

            Some(file) => file?.path(),
            None => return Err(FileError::NotFound)
        }).map_err(FileError::from)
    }

    pub fn info(&self) -> Result<String, FileError> {
        Ok(format!("
URL: {url}?file={}
File size: {}M
Days remaining: {}
", self.path.file_name().unwrap().to_str().unwrap(),
                self.size()? as f64 / 1000000.0, self.days()?,
   url = *crate::URL
        ))
    }

    fn days(&self) -> Result<f64, FileError> {
        let size_fixed = if self.size()? < 1000000 {
            5.0 as f64
        } else {
            let tmp = self.size()? as f64 / 1000000.0;
            if tmp < 5.0 { 5.0 } else { tmp }
        };

        let days = 150.0 / size_fixed -
            Utc::now().sub(self.time()?).num_days() as f64;

        Ok(if days > 30.0 {
            30.0
        } else if days < 0.0 {
            0.0
        } else {
            days
        })
    }

    fn size(&self) -> Result<u64, FileError> {
        match fs::read_dir(&self.path)?.next() {
            Some(file) => {
                Ok(file?.metadata()?.len())
            },
            None => Err(FileError::NotFound)
        }
    }

    fn time(&self) -> Result<DateTime<Utc>, FileError> {
        match self.time {
            Some(time) => Ok(time),
            None => match self.named_file() {
                Ok(file) => Ok(file.path().file_name().unwrap().to_str()
                               .unwrap().parse::<DateTime<Utc>>().unwrap()),
                Err(e) => Err(e)
            }
        }
    }

    fn hashed(data: &Vec<u8>) -> String {
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish().to_string()
    }
}

impl<'a> FromData<'a> for File {
    type Error = FileError;
    type Owned = Vec<u8>;
    type Borrowed = Self::Owned;

    fn transform(_: &Request, data: Data) ->
            Transform<Outcome<Self::Owned, Self::Error>> {

        let mut buffer: Vec<u8> = Vec::new();
        let outcome = match data.open().read_to_end(&mut buffer) {
            Ok(_) => Success(buffer),
            Err(e) => Failure((Status::InternalServerError, FileError::from(e)))
        };
        Transform::Owned(outcome)
    }

    fn from_data(_: &Request, outcome: Transformed<'a, Self>) ->
            Outcome<Self, Self::Error> {

        ExpGuardian::default().check_hour().unwrap_or(());
        Success(File::new(outcome.owned()?))
    }
}

impl<'v> FromFormValue<'v> for File {
    type Error = FileError;

    fn from_form_value(form_value: &'v RawStr) -> Result<File, Self::Error> {
        ExpGuardian::default().check_hour().unwrap_or(());

        match form_value.parse::<String>() {
            Ok(path) if Regex::new(r"\D+").unwrap().find(&path).is_none() => {
                let path = path_with_prefix(path);
                if path.exists() {
                    Ok(File::at(path))
                } else {
                    Err(FileError::NotFound)
                }
            },
            _ => Err(FileError::NotFound),
        }
    }
}
