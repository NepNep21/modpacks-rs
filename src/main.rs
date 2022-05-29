#![allow(non_snake_case)]

use std::{
    env, 
    collections::HashMap, 
    io::{Read, Write, self}, 
    fs::{create_dir_all, File}, 
    sync::mpsc::{self, Sender}, 
    path::PathBuf
};

use json::JsonValue;
use sha1::{Sha1, Digest};
use threadpool::ThreadPool;
use ureq::Response;
use zip::ZipArchive;

fn main() {
    let mut args = env::args();
    let firstArg = args.nth(1).expect("Invalid usage (consider \"help\")");
    let mut threadCount: usize = 1;
    let mut skipped = false;
    if firstArg == "--threads" {
        threadCount = args.next().expect("Invalid usage").parse::<usize>().expect("--threads must be a number");
    } else {
        skipped = true;
    }
    match if skipped { firstArg } else { args.next().expect("Invalid usage (consider \"help\")") }.as_str() {
        "ftb" => {
            match args.next().expect("Invalid usage").as_str() {
                "recent" => {
                    match getPopular() {
                        Ok(packs) => {
                            println!("Recent modpacks:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "featured" => {
                    match getFeatured() {
                        Ok(packs) => {
                            println!("Featured modpacks:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "played" => {
                    match getPlayed() {
                        Ok(packs) => {
                            println!("Most played modpacks:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "installed" => {
                    match getInstalled() {
                        Ok(packs) => {
                            println!("Most installed modpacks:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "search" => {
                    let term = args.next().expect("Invalid usage");
                    match searchFTB(term) {
                        Ok(packs) => {
                            println!("Search results:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "download" => {
                    let pack = args.next().expect("Invalid usage");
                    let version = args.next().expect("Invalid usage");
                    downloadPack(pack, version, PackType::FTB, threadCount).expect("Failed to download FTB modpack");
                }
                _ => {
                    eprintln!("Invalid usage")
                }
            }
        }
        "cf" => {
            match args.next().expect("Invalid usage").as_str() {
                "search" => {
                    let term = args.next().expect("Invalid usage");
                    match searchCF(term) {
                        Ok(packs) => {
                            println!("Search results:");
                            for (pack, info) in packs {
                                printPack(pack, info);
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error);
                        }
                    }
                }
                "download" => {
                    let pack = args.next().expect("Invalid usage");
                    let version = args.next().expect("Invalid usage");
                    downloadPack(pack, version, PackType::CF, threadCount).expect("Failed to download curseforge modpack");
                }
                _ => {
                    eprintln!("Invalid usage");
                }
            }
        }
        "help" => {
            let usage = "Usage (optional: []): modpacks-rs [--threads n] (ftb|cf) verb\n\
Verbs:
FTB:
recent: Lists the most recently updated modpacks
featured: Lists the featured modpacks
played: Lists the most played modpacks
installed: Lists the most installed modpacks
(search term): Searches for modpacks related to a term
(download id (version|latest)): Downloads a version of a modpack or the latest one
            
Curseforge:
(search term): Searches for modpacks related to a term
(download id (version|latest)): Downloads a version of a modpack or the latest one";
            println!("{}", usage);
        }
        _ => {
            eprintln!("Invalid usage (consider \"help\")");
        }
    }
}

#[derive(PartialEq)]
enum PackType {
    CF,
    FTB
}

impl PackType {
    pub fn url(&self) -> &str {
        match self {
            Self::CF => "https://api.modpacks.ch/public/curseforge/",
            Self::FTB => "https://api.modpacks.ch/public/modpack/"
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::CF => "Curseforge",
            Self::FTB => "FTB"
        }
    }
}

fn downloadPack(id: String, mut version: String, packType: PackType, threads: usize) -> Result<(), String> {
    if version == "latest" {
        let manifest = ureq::get(&(packType.url().to_owned() + &id))
            .call()
            .map_err(|it| format!("Failed to get modpack manifest: {:?}", it))?;
        let decoded = manifest.into_string()
            .map_err(|it| format!("Failed to parse modpack manifest as string: {:?}", it))?;
        let parsed = json::parse(&decoded)
            .map_err(|it| format!("Failed to parse modpack version manifest as json: {:?}", it))?;
        if let JsonValue::Array(versions) = &parsed["versions"] {
            let versions = versions.iter();
            let handled: Vec<&JsonValue>;
            if packType == PackType::FTB {
                handled = versions.rev().collect();
            } else {
                handled = versions.collect();
            }
            match handled.first() {
                Some(new) => {
                    version = new["id"].to_string();
                }
                None => {
                    return Err("Failed to retrieve latest version".to_string());
                }
            }
        } else {
            return Err("\"versions\" is not an array".to_string());
        }
    }

    let url = packType.url().to_owned() + &id + "/" + &version;
    
    let manifestResp = ureq::get(&url)
        .set("User-Agent", "curl/7.83.1") // API returns empty url otherwise
        .call()
        .map_err(|it| format!("Failed to get modpack version manifest: {:?}", it))?;
    let decoded = manifestResp.into_string()
        .map_err(|it| format!("Failed to parse modpack version manifest as string: {:?}", it))?;
    let parsed = json::parse(&decoded)
        .map_err(|it| format!("Failed to parse modpack version manifest as json: {:?}", it))?;
    let mut pool: Option<ThreadPool> = None;
    let files = parsed["files"].members();
    let (send, recv) = mpsc::channel::<String>();
    if threads > 1 {
        pool = Some(ThreadPool::new(threads));
    }
    match pool {
        Some(pool) => {
            // This feels jank
            let arr = parsed["files"].clone();
            let files = arr.members();
            for file in files {
                let file = file.clone();
                let id = id.clone();
                let send = send.clone();
                pool.execute(move || downloadFileThreaded(file, &id, send));
            }
            drop(send); // Drops the original sender to prevent infinite waiting when the threads exit, the clone can still send messages
            recv.recv().expect_err("Failed to download files");
            pool.join();
        }
        None => {
            for file in files {
                if let Err(error) = downloadFile(file.clone(), &id) {
                    return Err(error);
                }
            }
        }
    }
    // Handle overrides
    if packType == PackType::CF {
        println!("Extracting overrides");
        let basePath = "./".to_owned() + &id + "/";
        let file = File::open(basePath.clone() + "overrides.zip")
            .map_err(|it| format!("Failed to open overrides file: {:?}", it))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|it| format!("Failed to parse overrides as zip: {:?}", it))?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let path = match file.enclosed_name() {
                Some(path) => {
                    let mut buf = PathBuf::from(&basePath);
                    buf.push(path);
                    let overridePath = "./".to_string() + &id + "/overrides/";
                    let mut newBuf = PathBuf::new();
                    if buf.starts_with(overridePath) {
                        let mut trimmed = false;
                        for component in buf.components() {
                            match component {
                                std::path::Component::CurDir => {
                                    newBuf.push("./");
                                }
                                std::path::Component::Normal(path) => {
                                    if path != "overrides" || trimmed {
                                        newBuf.push(path);
                                    }
                                    if path == "overrides" {
                                        trimmed = true;
                                    }
                                }
                                other => {
                                    return Err(format!("Invalid component: {:?}", other));
                                }
                            }
                        }
                    } else {
                        newBuf = buf;
                    }
                    newBuf
                }
                None => continue
            };
            println!("Extracting override: {:?}", path);
            if file.is_dir() {
                create_dir_all(path)
                    .map_err(|it| format!("Failed to create overrides directory: {:?}", it))?;
            } else {
                if !path.is_dir() {
                    let mut outFile = File::create(path)
                        .map_err(|it| format!("Failed to create output override file: {:?}", it))?;
                    io::copy(&mut file, &mut outFile)
                        .map_err(|it| format!("Failed to copy output override file: {:?}", it))?;
                }
            }
        }
    }

    Ok(())
}

fn downloadFileThreaded(file: JsonValue, id: &String, channel: Sender<String>) {
    if let Err(error) = downloadFile(file, id) {
        channel.send(error.clone()).expect(&format!("Failed to send error message: {}", error));
    }
}

fn downloadFile(file: JsonValue, id: &String) -> Result<(), String> {
    let hash = file["sha1"].to_string();
    let url = file["url"].to_string();
    let name = file["name"].to_string();
    let path = "./".to_owned() + id + "/" + &file["path"].to_string();
    println!("Downloading {}{}", path, name);
    let resp = ureq::get(&url)
        .call()
        .map_err(|it| format!("Failed to get modpack file: {:?}", it))?;
    let mut raw: Vec<u8> = vec![];
    resp.into_reader().read_to_end(&mut raw).expect("Failed to read all bytes");
    let mut hasher = Sha1::new();
    hasher.update(&raw);
    let result = hex::encode(hasher.finalize());

    if result != hash && !hash.is_empty() { // Expected hashes are empty sometimes
        return Err(format!("Mismatched hashes, expected: {} found: {}", hash, result));
    }

    create_dir_all(&path).map_err(|it| format!("Failed to create path: {:?}", it))?;
    let mut file = File::create(path + &name)
        .map_err(|it| format!("Failed to create file: {:?}", it))?;
    file.write(&raw).map_err(|it| format!("Failed to write to file: {:?}", it))?;
    println!("Finished downloading");
    Ok(())
}

fn printPack(id: String, pack: HashMap<&'static str, String>) {
    println!("{}: {}", id, pack["name"]);
    println!("Authors: {}", pack["authors"]);
    println!("Description:\n{}", pack["description"]);
    println!("Versions:\n{}\n", pack["versions"]);
}

fn getPackInfo(id: String, packType: PackType) -> Result<HashMap<&'static str, String>, String> {
    let path = packType.url();
    let resp = ureq::get(&(path.to_owned() + &id))
        .call()
        .map_err(|it| format!("Failed to GET {} modpack info: {:?}", packType.name(), it))?;
    let string = resp.into_string().map_err(|it| format!("Failed to parse info response as string: {:?}", it))?;
    let data = json::parse(&string)
        .map_err(|it| format!("Failed to parse info response as json: {:?}", it))?;
    let mut info: HashMap<&'static str, String> = HashMap::new();
    info.insert("name", data["name"].to_string());
    let mut authors0: Vec<String> = vec![];
    if let JsonValue::Array(authors) = &data["authors"] {
        for author in authors {
            for (key, value) in author.entries() {
                if key == "name" {
                    authors0.push(value.to_string());
                }
            }
        }
    }
    info.insert("authors", format!("{:?}", authors0));
    info.insert("description", data["description"].to_string());
    let versions0: &Vec<JsonValue>;
    if let JsonValue::Array(versions) = &data["versions"] {
        versions0 = versions;
    } else {
        return Err("\"versions\" is not an array".to_string());
    }
    let len = versions0.len();

    let mut versionString = String::new();
    let mut revVersions = versions0.iter().rev();

    if packType == PackType::FTB {
        for i in 0..len {
            if i == 3 {
                break;
            }
            let version = revVersions.next().unwrap();
            versionString += &version["id"].as_i64().unwrap().to_string();
            versionString += ": ";
            versionString += version["name"].as_str().unwrap();
            if i != len - 1 && i != 2 {
                versionString += "\n";
            }
        }
    } else {
        for i in 0..len {
            if i == 3 {
                break;
            }
            let version = &versions0[i];
            versionString += &version["id"].as_i64().unwrap().to_string();
            versionString += ": ";
            versionString += version["name"].as_str().unwrap();
            if i != len - 1 && i != 2 {
                versionString += "\n";
            }
        }
    }
    info.insert("versions", versionString);

    Ok(info)
}

fn getPopular() -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = ureq::get("https://api.modpacks.ch/public/modpack/updated/10")
        .call()
        .map_err(|it| format!("Failed to GET recent modpacks: {:?}", it))?;
    let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
    for pack in parsePacks(resp)? {
        info.insert(pack.clone(), getPackInfo(pack, PackType::FTB)?);
    }
    Ok(info)
}

fn getFeatured() -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = ureq::get("https://api.modpacks.ch/public/modpack/featured/10")
        .call()
        .map_err(|it| format!("Failed to GET featured modpacks: {:?}", it))?;
    let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
    for pack in parsePacks(resp)? {
        info.insert(pack.clone(), getPackInfo(pack, PackType::FTB)?);
    }
    Ok(info)
}

fn getPlayed() -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = ureq::get("https://api.modpacks.ch/public/modpack/popular/plays/10")
        .call()
        .map_err(|it| format!("Failed to GET most played modpacks: {:?}", it))?;
    let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
    for pack in parsePacks(resp)? {
        info.insert(pack.clone(), getPackInfo(pack, PackType::FTB)?);
    }
    Ok(info)
}

fn getInstalled() -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = ureq::get("https://api.modpacks.ch/public/modpack/popular/installs/10")
        .call()
        .map_err(|it| format!("Failed to GET most installed modpacks: {:?}", it))?;
    let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
    for pack in parsePacks(resp)? {
        info.insert(pack.clone(), getPackInfo(pack, PackType::FTB)?);
    }
    Ok(info)
}

fn search(term: String) -> Result<Response, String> {
    ureq::get(&format!("https://api.modpacks.ch/public/modpack/search/5?term={}", &term))
        .call()
        .map_err(|it| format!("Failed to GET modpack search: {:?}", it))
}

fn searchFTB(term: String) -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = search(term)?;
    let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
    for pack in parsePacks(resp)? {
        info.insert(pack.clone(), getPackInfo(pack, PackType::FTB)?);
    }
    Ok(info)
}

fn searchCF(term: String) -> Result<HashMap<String, HashMap<&'static str, String>>, String> {
    let resp = search(term)?;
    let string = resp.into_string().map_err(|it| format!("Failed to parse response as string: {:?}", it))?;
    let data = json::parse(&string)
        .map_err(|it| format!("Failed to parse response as json: {:?}", it))?;
    if let JsonValue::Array(arr) = &data["curseforge"] {
        let packs: Vec<String> = arr.iter().map(|it| it.to_string()).collect();
        let mut info: HashMap<String, HashMap<&'static str, String>> = HashMap::new();
        for pack in packs {
            info.insert(pack.clone(), getPackInfo(pack, PackType::CF)?);
        }
        Ok(info)
    } else {
        Err("Invalid format".to_string())
    }
}

fn parsePacks(resp: Response) -> Result<Vec<String>, String> {
    let string = resp.into_string().map_err(|it| format!("Failed to parse response as string: {:?}", it))?;
    let data = json::parse(&string)
        .map_err(|it| format!("Failed to parse response as json: {:?}", it))?;
    if let JsonValue::Array(arr) = &data["packs"] {
        Ok(arr.iter().map(|it| it.to_string()).collect())
    } else {
        Err("Invalid format".to_string())
    }
}