use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::{Error, Write};
use std::path::Path;

use nanoid::nanoid;
use rocket::{Data, Outcome, Request, State};
use rocket::http::ContentType;
use rocket::request::FromRequest;
use rocket::response::{Debug, NamedFile};
use rocket_contrib::templates::Template;
use rocket_multipart_form_data::{mime, MultipartFormData, MultipartFormDataError, MultipartFormDataField, MultipartFormDataOptions, RawField};

use crate::cli;

pub fn register_routes(rocket: rocket::Rocket) -> rocket::Rocket {
    rocket.mount("/i", routes![get_img, post_img])
}

pub struct HostHeader<'a>(pub &'a str);

impl<'a, 'r> FromRequest<'a, 'r> for HostHeader<'a> {
    type Error = ();

    fn from_request(request: &'a Request) -> rocket::request::Outcome<Self, Self::Error> {
        match request.headers().get_one("Host") {
            Some(h) => Outcome::Success(HostHeader(h)),
            None => Outcome::Forward(()),
        }
    }
}

#[get("/<filename>")]
fn get_img(config: State<cli::AppConfig>, filename: String) -> Result<NamedFile, io::Error> {
    NamedFile::open(Path::new(config.storage_path.as_str()).join(filename))
}

#[derive(Serialize)]
struct UploadTemplateContext {
    url: String,
}

#[post("/upload", data = "<data>")]
fn post_img(config: State<cli::AppConfig>, host: HostHeader, content_type: &ContentType, data: Data) -> Result<Template, Debug<io::Error>> {
    let hostname = host.0;
    let http_type = if config.tls { "https" } else { "http" }; // We can probably get it from request headers

    let img_field_name = "img";
    let image = get_multipart_field(content_type, data, img_field_name)?;

    let image_name: String;
    match image {
        RawField::Single(raw) => {
            let id = nanoid!(10);
            image_name = format!("{}.{}", id, get_extension(&raw.file_name));

            let mut file = File::create(Path::new(config.storage_path.as_str()).join(&image_name))?;
            file.write_all(&raw.raw)?;
            let ctx = UploadTemplateContext { url: format!("{}://{}/i/{}", http_type, hostname, &image_name) };
            Ok(Template::render("uploaded", &ctx))
        }
        RawField::Multiple(_) => unreachable!(),
    }
}

fn get_extension(filename: &Option<String>) -> String {
    match filename {
        Some(s) => {
            if let Some(os_filename) = Path::new(&s).extension().and_then(OsStr::to_str) {
                String::from(os_filename)
            } else {
                String::from("bin")
            }
        }
        None => String::from("bin")
    }
}

fn get_multipart_field(content_type: &ContentType, data: Data, field_name: &str) -> Result<RawField, Debug<io::Error>> {
    let mut options = MultipartFormDataOptions::new();
    options.allowed_fields.push(
        MultipartFormDataField::raw(field_name).size_limit(64 * 1024 * 1024).content_type_by_string(Some(mime::IMAGE_STAR)).unwrap(),
    );

    let mut multipart_form_data = match MultipartFormData::parse(content_type, data, options) {
        Ok(multipart_form_data) => multipart_form_data,
        Err(err) => {
            match err {
                MultipartFormDataError::DataTooLargeError(_) => {
                    return Err(Debug::from(Error::new(std::io::ErrorKind::InvalidInput, "Data too large")));
                }
                MultipartFormDataError::DataTypeError(_) => {
                    return Err(Debug::from(Error::new(std::io::ErrorKind::InvalidInput, "Data not an image")));
                }
                _ => panic!("{:?}", err),
            }
        }
    };
    if let Some(field) = multipart_form_data.raw.remove(field_name) {
        return Ok(field);
    };
    Err(Debug::from(Error::new(std::io::ErrorKind::NotFound, "Missing field")))
}