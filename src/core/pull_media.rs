use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;
use crate::core::adb_command::AdbArgs;
use crate::core::ext::{OutputExt, StrExt, VecExt};
use crate::core::selector::{resolve_device, run_adb_with};
use crate::core::r#const::{PULL, SHELL};
use std::process::exit;
use crate::core::config::Config;
use crate::core::destination::Destination;
use crate::core::strings::{DESTINATION, MEDIAS_NOT_FOUND};
use crate::core::util::ensure_parent_exists;

const TOYBOX_LS_LLCD: &str = "toybox ls -llcd";
const PICS: &[&str; 3] = &[".png", ".jpg", ".jpeg"];
const MOVS: &[&str; 3] = &[".mp4", ".mov", ".3gp"];
// -rw-rw---- 1 u0_a173 media_rw 6217184 2023-10-23 00:15:07.020796477 +0200(wtf?) /sdcard/Pictures/Screenshots/screenshot.png
const PART_MIN_COUNT: usize = 8;
const PART_DATE: usize = 5;
const PART_TIME: usize = 6;


pub enum Params {
    Count(usize),
    Single(String),
}

impl Display for Params {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Params::Count(count) => write!(f, "Params::Count({count})"),
            Params::Single(path) => write!(f, "Params::Single(\"{path})\""),
        }
    }
}

pub fn pull_screenshots(params: Params) {
    let config = Config::read().screenshots;
    let command = get_ls_command(&config.sources);
    pull(params, PICS, &[SHELL, command.as_str()], config.destination);
}

pub fn pull_screencasts(params: Params) {
    let config = Config::read().screencasts;
    let command = get_ls_command(&config.sources);
    pull(params, MOVS, &[SHELL, command.as_str()], config.destination);
}

fn get_ls_command(sources: &Vec<String>) -> String {
    let mut command = TOYBOX_LS_LLCD.to_string();
    for src in sources {
        let slash = if src.ends_with('/') { "" } else { "/" };
        let part = format![" {src}{slash}*"];
        command.push_str(part.as_str())
    }
    return command;
}

fn pull(params: Params, exts: &[&str], args: &[&str], default_dst: String) {
    let count = match params {
        Params::Count(count) => count,
        Params::Single(_) => 1,
    };
    if count <= 0 {
        return
    }
    let device = resolve_device();
    let output = run_adb_with(&device, AdbArgs::run(args));
    let mut items = output.stdout().split('\n')
        .into_iter()
        .map(|it| splitn_by(it, PART_MIN_COUNT, ' '))
        .filter_map(|it| as_item_or_none(exts, it))
        .collect::<Vec<Item>>();
    if items.is_empty() {
        let err = output.stderr();
        if err.is_empty() {
            MEDIAS_NOT_FOUND.println();
        } else {
            output.print_err();
            exit(output.code());
        }
    } else {
        items.sort();
        let mut items = items.iter()
            .take(count)
            .map(|it| it.path.to_string())
            .collect::<Vec<String>>();
        let dst = match params {
            Params::Count(_) => {
                let dst = default_dst.with_dir("");
                fs::create_dir_all(&dst).unwrap();
                dst
            },
            Params::Single(path) => {
                let name = Path::new(items.first().unwrap())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap();
                let dst = path
                    .with_dir(&default_dst)
                    .with_file(name);
                ensure_parent_exists(&dst);
                dst
            },
        };
        let mut pull_args = AdbArgs::spawn(&[PULL]);
        items.reverse();
        pull_args.args.append(&mut items);
        pull_args.args.push(dst.clone());
        let output = run_adb_with(&device, pull_args);
        output.print_out_and_err();
        if output.status.success() {
            DESTINATION.print();
            println!("{dst}");
        }
        exit(output.code());
    }
}

fn as_item_or_none(exts: &[&str], line: Vec<String>) -> Option<Item> {
    match () {
        _ if line.len() < PART_MIN_COUNT => return None,
        _ if line[0].chars().next() != Some('-') => return None,
        _ => (),
    }
    let last = line[line.last_index()].to_string();
    let dot = last.last_index_of('.');
    let ext = match dot {
        Some(index) => last[index..].to_lowercase(),
        None => return None,
    };
    if !exts.contains(&ext.as_str()) {
        return None
    }
    let date = line[PART_DATE].to_string();
    let time = line[PART_TIME].to_string();
    let root = last.chars().position(|c| c == '/').unwrap();
    // ignore the part contains '+0200' if exists
    let path = last[root..].to_string();
    Some(Item { date, time, path })
}

fn splitn_by(str: &str, limit: usize, sep: char) -> Vec<String> {
    let mut parts: Vec<String> = vec![];
    let mut buf: Vec<char> = vec![];
    let release_part = |buf: &mut Vec<char>, parts: &mut Vec<String>| {
        parts.push(buf.clone().into_iter().collect::<String>());
        buf.clear();
    };
    for c in str.chars() {
        if c != sep || parts.len() == (limit - 1) {
            buf.push(c);
        } else if !buf.is_empty() {
            release_part(&mut buf, &mut parts);
        }
    }
    if !buf.is_empty() {
        release_part(&mut buf, &mut parts);
    }
    return parts;
}

struct Item {
    pub date: String,
    pub time: String,
    pub path: String,
}

impl Eq for Item {}

impl PartialEq<Self> for Item {
    fn eq(&self, other: &Self) -> bool {
        self.date == other.date && self.time == other.time
    }
}

impl PartialOrd<Self> for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        match () {
            _ if self.date > other.date => Ordering::Less,
            _ if self.date < other.date => Ordering::Greater,
            _ if self.time > other.time => Ordering::Less,
            _ if self.time < other.time => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}
