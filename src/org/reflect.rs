//! gRPC server reflection (`grpc.reflection.v1` and `v1alpha`) for executables: the shared
//! `muninn/code/v1/executable.proto` and each served space anyone may see, as
//! `FileDescriptorProto`s built here (no protoc, no prost-types). Reflection answers as a
//! guest: a private org's services are found through its `/o/ORG/proto/` files instead.
use super::exec::{self, COMMON_PROTO, Executable, LIST_SERVICE, PACKAGE};
use crate::store::{Org, Store};
use prost::Message;

pub const SERVICES: &[&str] = &["grpc.reflection.v1.ServerReflection", "grpc.reflection.v1alpha.ServerReflection"];

/// Is this the reflection stream's path?
pub fn is_reflection(path: &str) -> bool {
    SERVICES.iter().any(|s| path == format!("/{s}/ServerReflectionInfo"))
}

// ---------- descriptor.proto, the parts used ----------

#[derive(Clone, PartialEq, Message)]
pub struct FieldDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub number: Option<i32>,
    #[prost(int32, optional, tag = "4")]
    pub label: Option<i32>,
    #[prost(int32, optional, tag = "5")]
    pub r#type: Option<i32>,
    #[prost(string, optional, tag = "6")]
    pub type_name: Option<String>,
    #[prost(string, optional, tag = "10")]
    pub json_name: Option<String>,
}
#[derive(Clone, PartialEq, Message)]
pub struct DescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub field: Vec<FieldDescriptorProto>,
}
#[derive(Clone, PartialEq, Message)]
pub struct MethodDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub input_type: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub output_type: Option<String>,
}
#[derive(Clone, PartialEq, Message)]
pub struct ServiceDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub method: Vec<MethodDescriptorProto>,
}
#[derive(Clone, PartialEq, Message)]
pub struct FileDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub package: Option<String>,
    #[prost(string, repeated, tag = "3")]
    pub dependency: Vec<String>,
    #[prost(message, repeated, tag = "4")]
    pub message_type: Vec<DescriptorProto>,
    #[prost(message, repeated, tag = "6")]
    pub service: Vec<ServiceDescriptorProto>,
    #[prost(string, optional, tag = "12")]
    pub syntax: Option<String>,
}

// ---------- reflection.proto ----------

#[derive(Clone, PartialEq, Message)]
pub struct ReflectionRequest {
    #[prost(string, tag = "1")]
    pub host: String,
    #[prost(string, optional, tag = "3")]
    pub file_by_filename: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub file_containing_symbol: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub list_services: Option<String>,
}
#[derive(Clone, PartialEq, Message)]
pub struct FileDescriptorResponse {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub file_descriptor_proto: Vec<Vec<u8>>,
}
#[derive(Clone, PartialEq, Message)]
pub struct ServiceResponse {
    #[prost(string, tag = "1")]
    pub name: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct ListServiceResponse {
    #[prost(message, repeated, tag = "1")]
    pub service: Vec<ServiceResponse>,
}
#[derive(Clone, PartialEq, Message)]
pub struct ErrorResponse {
    #[prost(int32, tag = "1")]
    pub error_code: i32,
    #[prost(string, tag = "2")]
    pub error_message: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct ReflectionResponse {
    #[prost(string, tag = "1")]
    pub valid_host: String,
    #[prost(message, optional, tag = "2")]
    pub original_request: Option<ReflectionRequest>,
    #[prost(message, optional, tag = "4")]
    pub file_descriptor_response: Option<FileDescriptorResponse>,
    #[prost(message, optional, tag = "6")]
    pub list_services_response: Option<ListServiceResponse>,
    #[prost(message, optional, tag = "7")]
    pub error_response: Option<ErrorResponse>,
}

// ---------- the descriptors ----------

const DOUBLE: i32 = 1;
const INT64: i32 = 3;
const BOOL: i32 = 8;
const STRING: i32 = 9;
const MESSAGE: i32 = 11;
const UINT32: i32 = 13;

/// (message, [(field, number, type, repeated, message type)]) as in `exec::common_proto`.
type Spec = (&'static str, &'static [(&'static str, i32, i32, bool, &'static str)]);
const COMMON: &[Spec] = &[
    (
        "Field",
        &[
            ("name", 1, STRING, false, ""),
            ("number", 2, UINT32, false, ""),
            ("type", 3, STRING, false, ""),
            ("repeated", 4, BOOL, false, ""),
            ("doc", 5, STRING, false, ""),
        ],
    ),
    ("Scope", &[("kind", 1, STRING, false, ""), ("org_id", 2, STRING, false, "")]),
    (
        "Executable",
        &[
            ("id", 1, STRING, false, ""),
            ("name", 2, STRING, false, ""),
            ("service", 3, STRING, false, ""),
            ("language", 4, STRING, false, ""),
            ("version_hash", 5, STRING, false, ""),
            ("scope", 6, MESSAGE, false, "Scope"),
            ("inputs", 7, MESSAGE, true, "Field"),
            ("output", 8, STRING, false, ""),
            ("proto", 9, STRING, false, ""),
        ],
    ),
    ("ListRequest", &[("org", 1, STRING, false, "")]),
    ("ExecutableList", &[("executables", 1, MESSAGE, true, "Executable")]),
    ("Row", &[("cells", 1, STRING, true, "")]),
    ("Series", &[("name", 1, STRING, false, ""), ("values", 2, DOUBLE, true, "")]),
    (
        "View",
        &[
            ("kind", 1, STRING, false, ""),
            ("title", 2, STRING, false, ""),
            ("columns", 3, STRING, true, ""),
            ("rows", 4, MESSAGE, true, "Row"),
            ("x", 5, MESSAGE, false, "Series"),
            ("x_time", 6, BOOL, false, ""),
            ("series", 7, MESSAGE, true, "Series"),
            ("text", 8, STRING, false, ""),
            ("tiles", 9, MESSAGE, true, "View"),
            ("dir", 10, STRING, false, ""),
        ],
    ),
    (
        "RunResult",
        &[
            ("status", 1, STRING, false, ""),
            ("error", 2, STRING, false, ""),
            ("view", 3, MESSAGE, false, "View"),
            ("logs", 4, STRING, true, ""),
            ("writes", 5, INT64, false, ""),
            ("ms", 6, INT64, false, ""),
            ("json", 7, STRING, false, ""),
        ],
    ),
];

fn camel(n: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in n.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.push(c.to_ascii_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn field(name: &str, number: i32, ty: i32, repeated: bool, type_name: Option<String>) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.into()),
        number: Some(number),
        label: Some(if repeated { 3 } else { 1 }),
        r#type: Some(ty),
        type_name,
        json_name: Some(camel(name)),
    }
}

fn method(name: &str, input: String, output: String) -> MethodDescriptorProto {
    MethodDescriptorProto { name: Some(name.into()), input_type: Some(input), output_type: Some(output) }
}

pub fn common_file() -> FileDescriptorProto {
    let message_type = COMMON
        .iter()
        .map(|(m, fs)| DescriptorProto {
            name: Some((*m).into()),
            field: fs.iter().map(|(n, num, ty, rep, tn)| field(n, *num, *ty, *rep, (*ty == MESSAGE).then(|| format!(".{PACKAGE}.{tn}")))).collect(),
        })
        .collect();
    FileDescriptorProto {
        name: Some(COMMON_PROTO.into()),
        package: Some(PACKAGE.into()),
        dependency: vec![],
        message_type,
        service: vec![ServiceDescriptorProto {
            name: Some("Executables".into()),
            method: vec![method("List", format!(".{PACKAGE}.ListRequest"), format!(".{PACKAGE}.ExecutableList"))],
        }],
        syntax: Some("proto3".into()),
    }
}

fn short(e: &Executable) -> &str {
    e.service.rsplit('.').next().unwrap_or(&e.service)
}

/// A space's file name: `muninn/code/v1/o/<org>/<Service>.proto`.
pub fn space_file_name(e: &Executable) -> String {
    let pkg = e.service.rsplit_once('.').map(|p| p.0).unwrap_or_default();
    format!("{}/{}.proto", pkg.replace('.', "/"), short(e))
}

pub fn space_file(org: &Org, e: &Executable) -> FileDescriptorProto {
    let pkg = exec::package(org);
    let svc = short(e);
    let ty = |k: &str| if k == "int64" { INT64 } else { STRING };
    FileDescriptorProto {
        name: Some(space_file_name(e)),
        package: Some(pkg.clone()),
        dependency: vec![COMMON_PROTO.into()],
        message_type: vec![DescriptorProto {
            name: Some(format!("{svc}Input")),
            field: e.inputs.iter().map(|f| field(f.name, f.number as i32, ty(f.kind), f.repeated, None)).collect(),
        }],
        service: vec![ServiceDescriptorProto {
            name: Some(svc.into()),
            method: vec![method("Run", format!(".{pkg}.{svc}Input"), format!(".{PACKAGE}.RunResult"))],
        }],
        syntax: Some("proto3".into()),
    }
}

/// Every executable a guest may see, with its org.
fn public(s: &Store) -> Vec<(Org, Executable)> {
    let mut out = vec![];
    for org in s.orgs().unwrap_or_default() {
        let Some(a) = super::serve::exec_caller(s, &org, None) else { continue };
        for e in exec::executables(s, &org, &a).unwrap_or_default() {
            out.push((org.clone(), e));
        }
    }
    out
}

/// One reflection request's response.
pub fn answer(s: &Store, msg: &[u8]) -> std::result::Result<Vec<u8>, (u32, String)> {
    let q = ReflectionRequest::decode(msg).map_err(|e| (3, format!("bad message: {e}")))?;
    let mut r = ReflectionResponse { valid_host: q.host.clone(), original_request: Some(q.clone()), ..Default::default() };
    let files = |fs: Vec<FileDescriptorProto>| Some(FileDescriptorResponse { file_descriptor_proto: fs.iter().map(|f| f.encode_to_vec()).collect() });
    let missing = |what: &str| Some(ErrorResponse { error_code: 5, error_message: format!("{what} not found") });
    if q.list_services.is_some() {
        let mut names: Vec<String> = vec![LIST_SERVICE.into()];
        names.extend(SERVICES.iter().map(|s| s.to_string()));
        names.extend(public(s).into_iter().map(|(_, e)| e.service));
        r.list_services_response = Some(ListServiceResponse { service: names.into_iter().map(|name| ServiceResponse { name }).collect() });
    } else if let Some(f) = &q.file_by_filename {
        if f == COMMON_PROTO {
            r.file_descriptor_response = files(vec![common_file()]);
        } else {
            match public(s).into_iter().find(|(_, e)| &space_file_name(e) == f) {
                Some((o, e)) => r.file_descriptor_response = files(vec![space_file(&o, &e), common_file()]),
                None => r.error_response = missing(f),
            }
        }
    } else if let Some(sym) = &q.file_containing_symbol {
        let sym = sym.trim_start_matches('.');
        let own = format!("{PACKAGE}.o.");
        if sym.starts_with(&format!("{PACKAGE}.")) && !sym.starts_with(&own) {
            r.file_descriptor_response = files(vec![common_file()]);
        } else {
            let hit = public(s)
                .into_iter()
                .find(|(_, e)| sym == e.service || sym.starts_with(&format!("{}.", e.service)) || sym == format!("{}Input", e.service));
            match hit {
                Some((o, e)) => r.file_descriptor_response = files(vec![space_file(&o, &e), common_file()]),
                None => r.error_response = missing(sym),
            }
        }
    } else {
        r.error_response = Some(ErrorResponse { error_code: 12, error_message: "not supported".into() });
    }
    Ok(r.encode_to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_file_matches_its_text() {
        let f = common_file();
        let text = exec::common_proto();
        for m in &f.message_type {
            let name = m.name.as_deref().unwrap();
            assert!(text.contains(&format!("message {name} {{")), "{name}");
            for fd in &m.field {
                assert!(text.contains(&format!(" {} = {};", fd.name.as_deref().unwrap(), fd.number.unwrap())), "{name}.{:?}", fd.name);
            }
        }
        assert_eq!(FileDescriptorProto::decode(f.encode_to_vec().as_slice()).unwrap(), f);
    }

    #[test]
    fn lists_services_and_finds_files() {
        let s = Store::open_in_memory().unwrap();
        let ask = |q: ReflectionRequest| ReflectionResponse::decode(answer(&s, &q.encode_to_vec()).unwrap().as_slice()).unwrap();
        let l = ask(ReflectionRequest { list_services: Some(String::new()), ..Default::default() });
        let names: Vec<_> = l.list_services_response.unwrap().service.into_iter().map(|s| s.name).collect();
        assert!(names.contains(&LIST_SERVICE.to_string()) && names.contains(&SERVICES[0].to_string()), "{names:?}");
        let f = ask(ReflectionRequest { file_containing_symbol: Some(format!("{LIST_SERVICE}.List")), ..Default::default() });
        let got = FileDescriptorProto::decode(f.file_descriptor_response.unwrap().file_descriptor_proto[0].as_slice()).unwrap();
        assert_eq!(got.name.as_deref(), Some(COMMON_PROTO));
        let miss = ask(ReflectionRequest { file_by_filename: Some("nope.proto".into()), ..Default::default() });
        assert_eq!(miss.error_response.unwrap().error_code, 5);
        assert!(is_reflection("/grpc.reflection.v1.ServerReflection/ServerReflectionInfo") && !is_reflection("/x/y"));
    }
}
