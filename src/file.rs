use regex::Regex;
use chrono::prelude::*;
use rocket::{Data, Outcome::*};
use rocket::request::{Request, FromFormValue};
use rocket::data::{FromData, Outcome, Transform, Transformed};
use rocket::response::NamedFile;
use rocket::http::{Status, RawStr};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::fs;
use std::path::PathBuf;
use std::env::var;

#[derive(Debug, Fail)]
pub enum FileError {
    #[fail(display = "Sorry, no file for you.")]
    NotFound,

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
        FileError::Internal{ err: error.description().to_string() }
    }
}

fn path_with_prefix(path: String) -> PathBuf {
    let mut path_pref = PathBuf::from(var("SAVE_DIR").unwrap_or(String::new()));
    path_pref.push(path);
    path_pref
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
        if path.exists() {
            File::at(path)
        } else {
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
        if self.data.as_ref().unwrap().len() > 150000000 {
            return Ok("I don't accept fat files. file_size > 150m"
                      .to_string());
        }

        if self.path.exists() {
            return Ok(
                "Someone has already uploaded this file before. \
                No need to recreate it.\r".to_string() + &self.info()?);
        } else {
            fs::create_dir_all(&self.path).map_err(FileError::from)?;
        }

        let mut pfile = PathBuf::from(&self.path);
        pfile.push(PathBuf::from(&self.time()?.to_rfc3339()));
        match fs::File::create(pfile).map_err(FileError::from)?
                .write_all(&self.data.as_ref().unwrap()[..]) {

            Ok(_) => Ok(self.info()?),
            Err(e) => Err(FileError::from(e))
        }
    }

    pub fn delete(&self) -> Result<(), FileError> {
        fs::remove_dir_all(match self.path.parent() {
            Some(folder) => folder,
            None => return Err(FileError::NotFound)
        }).map_err(FileError::from)
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
                self.size()? / 1000000, self.days()?,
   url = crate::url()
        ))
    }

    fn days(&self) -> Result<f64, FileError> {
        use std::ops::Sub;

        match self.size() {
            Ok(size) =>  {
                let mut days = 150.0 / (size as f64 / 1000000.0) -
                    ((self.time()?.sub(Utc::now())).num_days() as f64);
                days = if days > 30.0 {
                    30.0
                } else if days < 1.0 {
                    1.0
                } else {
                    days
                };
                Ok(days)
            },
            Err(e) => Err(FileError::from(e))
        }
    }

    fn size(&self) -> Result<u64, FileError> {
        match fs::read_dir(&self.path)?.next() {
            Some(file) => Ok(file?.metadata()?.len()),
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

        Success(File::new(outcome.owned()?))
    }
}

impl<'v> FromFormValue<'v> for File {
    type Error = FileError;

    fn from_form_value(form_value: &'v RawStr) -> Result<File, Self::Error> {
        match form_value.parse::<String>() {
            Ok(path) if Regex::new(r"\D+").unwrap().find(&path).is_none() => {
                let path = path_with_prefix(path);
                Ok(File::at(path))
            },
            _ => Err(FileError::NotFound),
        }
    }
}
