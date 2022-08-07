#![allow(non_snake_case)]

use std::{
    env, 
    collections::HashMap, 
    io::{Read, Write, self}, 
    fs::{create_dir_all, File}, 
    sync::mpsc::{self, Sender}, 
    path::PathBuf, 
    process::Command
};

#[cfg(not(windows))]
use std::{fs::{Permissions, self}, os::unix::prelude::PermissionsExt};

use json::JsonValue;
use roxmltree::Document;
use sha1::{Sha1, Digest};
use threadpool::ThreadPool;
use ureq::Response;
use zip::ZipArchive;

const USER_AGENT: &str = "modpacklauncher/202207271710-0f9644f5fc-release Mozilla/5.0 (LINUX) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/57.0.2987.138 Safari/537.36 Vivaldi/1.8.770.56";

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
                    downloadPack(&pack, version, PackType::FTB, threadCount).expect("Failed to download FTB modpack");
                }
                "server" => {
                    let pack = args.next().expect("Invalid usage");
                    let version = args.next().expect("Invalid usage");
                    downloadFTBServer(pack, version).expect("Failed to install server");
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
                    downloadPack(&pack, version, PackType::CF, threadCount).expect("Failed to download curseforge modpack");
                }
                "server" => {
                    let pack = args.next().expect("Invalid usage");
                    let version = args.next().expect("Invalid usage");
                    downloadCFServer(pack, version, threadCount).expect("Failed to install server");
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
(server id (version|latest)): Downloads a version of a server or the latest one
            
Curseforge:
(search term): Searches for modpacks related to a term
(server id (version|latest)): Downloads a version of a modpack or the latest one and performs a server installation
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

// More targets may be added with requests
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn getFTBServerURL(id: &String, version: &String) -> String {
    format!("https://api.modpacks.ch/public/modpack/{}/{}/server/linux", id, version)
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
fn getFTBServerURL(id: &String, version: &String) -> String {
    format!("https://api.modpacks.ch/public/modpack/{}/{}/server/windows", id, version)
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn getFTBServerURL(id: &String, version: &String) -> String {
    format!("https://api.modpacks.ch/public/modpack/{}/{}/server/arm/linux", id, version)
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn getFTBServerURL(id: &String, version: &String) -> String {
    format!("https://api.modpacks.ch/public/modpack/{}/{}/server/arm/mac", id, version)
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
fn getFTBServerURL(id: &String, version: &String) -> String {
    format!("https://api.modpacks.ch/public/modpack/{}/{}/server/mac", id, version)
}

#[cfg(not(target_os = "windows"))]
fn getInstallerName(base: String) -> String {
    base + "installer"
}

#[cfg(target_os = "windows")]
fn getInstallerName(base: String) -> String {
    base + "installer.exe" // In case the user wants to run it later
}

fn tryRunJava(javaArgs: &[&str], typeName: &str) -> Result<(), String> {
    match Command::new("java").args(javaArgs).spawn() {
        Ok(mut proc) => {
            proc.wait().map_err(|it| format!("Failed to wait for forge installer: {:?}", it))?;
        }
        Err(_) => {
            // Assuming file not found
            let javaPath = env::var("JAVA_HOME")
                .map(|path| path + &if cfg!(windows) { "/bin/java.exe" } else { "/bin/java" })
                .map_err(|it| format!("Failed to find java for running installer: {:?}", it))?;
            Command::new(javaPath)
                .args(javaArgs)
                .spawn()
                .map_err(|it| format!("Failed to spawn {typeName} installer: {:?}", it))?
                .wait()
                .map_err(|it| format!("Failed to wait for {typeName} installer: {:?}", it))?;
        }
    }
    Ok(())
}

fn downloadCFServer(id: String, mut version: String, threads: usize) -> Result<(), String> {
    if version == "latest" {
        version = getLatestVersion(&id, &PackType::CF)?;
    }
    downloadPack(&id, version, PackType::CF, threads)?;
    let mut file = File::open(format!("./{}/manifest.json", id))
        .map_err(|it| format!("Failed to open manifest: {:?}", it))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|it| format!("Failed to read manifest: {:?}", it))?;
    let manifest = json::parse(&buf).map_err(|it| format!("Manifest is invalid: {:?}", it))?;
    let mcSection = &manifest["minecraft"];
    let mcVersion = &mcSection["version"].to_string();
    let loader = &mcSection["modLoaders"][0]["id"].to_string();
    let mut split = loader.split('-');
    let name = split.next().unwrap();
    let version = split.next().unwrap();

    // Maybe quilt support at some point?
    match name {
        "forge" => {
            let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mcVersion}-{version}/forge-{mcVersion}-{version}-installer.jar");
            let resp = ureq::get(&url)
                .call()
                .map_err(|it| format!("Failed to download forge installer: {:?}", it))?;
            let mut raw: Vec<u8> = vec![];
            resp.into_reader()
                .read_to_end(&mut raw)
                .map_err(|it| format!("Failed to read all bytes: {:?}", it))?;
            env::set_current_dir(format!("./{id}")).map_err(|it| format!("Failed to cd: {:?}", it))?;
            let path = format!("./installer.jar");
            let mut installer = File::create(&path)
                .map_err(|it| format!("Couldn't save installer: {:?}", it))?;
            installer.write_all(&raw).map_err(|it| format!("Couldn't save installer: {:?}", it))?;
            let javaArgs = ["-jar", &path, "--installServer"];
            return tryRunJava(&javaArgs, "forge");
        }
        "fabric" => {
            let mavenMeta = ureq::get("https://maven.fabricmc.net/net/fabricmc/fabric-installer/maven-metadata.xml")
                .call()
                .map_err(|it| format!("Failed to GET fabric installer maven metadata: {:?}", it))?
                .into_string()
                .map_err(|it| format!("Failed to parse maven metadata as string: {:?}", it))?;
            let xml = Document::parse(&mavenMeta)
                .map_err(|it| format!("Failed to parse metadata as xml: {:?}", it))?;
            let latestVersion = xml.descendants()
                .filter(|node| node.has_tag_name("release"))
                .next()
                .map(|node| node.text());
            match latestVersion {
                Some(opt) => {
                    match opt {
                        Some(loaderVersion) => {
                            let url = format!("https://maven.fabricmc.net/net/fabricmc/fabric-installer/{loaderVersion}/fabric-installer-{loaderVersion}.jar");
                            let resp = ureq::get(&url)
                                .call()
                                .map_err(|it| format!("Failed to download fabric installer: {:?}", it))?;
                            let mut raw: Vec<u8> = vec![];
                            resp.into_reader()
                                .read_to_end(&mut raw)
                                .map_err(|it| format!("Failed to read all bytes: {:?}", it))?;
                            let dir = format!("./{id}");
                            let path = format!("{dir}/installer.jar");
                            let mut installer = File::create(&path)
                                .map_err(|it| format!("Couldn't save installer: {:?}", it))?;
                            installer.write_all(&raw).map_err(|it| format!("Couldn't save installer: {:?}", it))?;
                            let args = &["-jar", &path, "server", "-dir", &dir, "-mcversion", &mcVersion, "-loader", &version, "-downloadMinecraft"];
                            return tryRunJava(args, "fabric");
                        }
                        None => {
                            return Err("Invalid maven metadata".to_string());
                        }
                    }
                }
                None => {
                    return Err("Invalid maven metadata".to_string());
                }
            }
        }
        other => {
            return Err(format!("Unsupported modloader: {}", other));
        }
    }
}

#[cfg(not(windows))]
fn makeExecutable(path: &String, file: &File) -> Result<(), String> {
    let mode = fs::metadata(path)
        .map_err(|it| format!("Failed to get file mode: {:?}", it))?
        .permissions()
        .mode();
    let mode = format!("{:o}", mode);
    let mut mode: Vec<char> = mode.chars().collect();
    mode[3] = '7';
    let mode = mode.iter().collect::<String>();
    let mode = u32::from_str_radix(&mode, 8).unwrap();
    file.set_permissions(Permissions::from_mode(mode)).unwrap();
    Ok(())
}

fn downloadFTBServer(id: String, mut version: String) -> Result<(), String> {
    if version == "latest" {
        version = getLatestVersion(&id, &PackType::FTB)?;
    }
    let url = getFTBServerURL(&id, &version);
    let resp = ureq::get(&url)
        .call()
        .map_err(|it| format!("Failed to download installer: {:?}", it))?;
    let mut raw: Vec<u8> = vec![];
    resp.into_reader().read_to_end(&mut raw).expect("Failed to read all bytes");
    let basePath = "./".to_string() + &id + "/";
    create_dir_all(&basePath)
        .map_err(|it| format!("Failed to create server directory: {:?}", it))?;
    let installerName = getInstallerName(basePath);
    let mut file = File::create(&installerName)
        .map_err(|it| format!("Failed to create server file: {:?}", it))?;
    file.write(&raw).map_err(|it| format!("Failed to write to server file: {:?}", it))?;

    #[cfg(not(windows))]
    makeExecutable(&installerName, &file)?;

    drop(file); // Otherwise spawning won't work
    Command::new(installerName)
        .args([&id, &version, "--auto", "--path", &id])
        .spawn()
        .map_err(|it| format!("Failed to spawn installer: {:?}", it))?
        .wait()
        .map_err(|it| format!("Failed to wait for installer to complete: {:?}", it))?;
    Ok(())
}

fn getLatestVersion(id: &String, packType: &PackType) -> Result<String, String> {
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
            if packType == &PackType::FTB {
                handled = versions.rev().collect();
            } else {
                handled = versions.collect();
            }
            match handled.first() {
                Some(new) => {
                    return Ok(new["id"].to_string());
                }
                None => {
                    return Err("Failed to retrieve latest version".to_string());
                }
            }
        } else {
            return Err("\"versions\" is not an array".to_string());
        }
}

fn downloadPack(id: &String, mut version: String, packType: PackType, threads: usize) -> Result<(), String> {
    if version == "latest" {
        version = getLatestVersion(&id, &packType)?;
    }

    let url = packType.url().to_owned() + &id + "/" + &version;
    
    let manifestResp = ureq::get(&url)
        .set("User-Agent", USER_AGENT) // API returns empty url otherwise
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
                    if let Some(parent) = path.parent() {
                        create_dir_all(parent)
                            .map_err(|it| format!("Failed to create overrides directory: {:?}", it))?;
                    }
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
