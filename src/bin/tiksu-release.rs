//! Release signing for Tiksu Clipper updates.
//!
//!   tiksu-release keygen <secret-key-file>
//!       Creates a signing key (keep it secret, back it up) and prints the public key
//!       that goes into `PUBLIC_KEY` in src/update.rs.
//!
//!   tiksu-release verify --exe <file> --manifest <latest.json> --public-key <hex>
//!       Checks a published exe against its manifest (e.g. after a release).
//!
//!   tiksu-release sign --exe <file> --version <x.y.z> --url <download url> [--notes-file <file>] [--key <file>] --out <latest.json>
//!       Signs the exe and writes the update manifest. The key comes from --key or the
//!       TIKSU_SIGNING_KEY environment variable (hex), e.g. a GitHub Actions secret.

#[path = "../manifest.rs"]
mod manifest;

use ed25519_dalek::{Signer, SigningKey};
use manifest::{from_hex, parse_version, signed_message, to_hex, Manifest};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(args.get(1)),
        Some("sign") => sign(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => Err("käyttö: tiksu-release keygen <tiedosto> | sign --exe .. --version .. --url .. --out ..".to_string()),
    };
    if let Err(e) = result {
        eprintln!("virhe: {}", e);
        std::process::exit(1);
    }
}

fn keygen(path: Option<&String>) -> Result<(), String> {
    let path = path.ok_or("anna avaintiedoston polku")?;
    if std::path::Path::new(path).exists() {
        return Err(format!("{} on jo olemassa; vanhaa avainta ei korvata", path));
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| e.to_string())?;
    let key = SigningKey::from_bytes(&seed);
    std::fs::write(path, to_hex(&seed)).map_err(|e| e.to_string())?;
    println!("Salainen avain: {}", path);
    println!("Julkinen avain (src/update.rs PUBLIC_KEY): {}", to_hex(key.verifying_key().as_bytes()));
    Ok(())
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn sign(args: &[String]) -> Result<(), String> {
    let need = |name| arg(args, name).ok_or(format!("{} puuttuu", name));
    let (exe_path, version, url, out) = (need("--exe")?, need("--version")?, need("--url")?, need("--out")?);
    parse_version(version).ok_or(format!("versio \"{}\" ei ole muotoa x.y.z", version))?;

    let key_hex = match arg(args, "--key") {
        Some(file) => std::fs::read_to_string(file).map_err(|e| format!("{}: {}", file, e))?,
        None => std::env::var("TIKSU_SIGNING_KEY").map_err(|_| "anna --key tai TIKSU_SIGNING_KEY")?,
    };
    let seed: [u8; 32] = from_hex(&key_hex)
        .and_then(|k| k.try_into().ok())
        .ok_or("avain ei ole 32 tavun hex-merkkijono")?;
    let key = SigningKey::from_bytes(&seed);

    let exe = std::fs::read(exe_path).map_err(|e| format!("{}: {}", exe_path, e))?;
    let notes = match arg(args, "--notes-file") {
        Some(file) => std::fs::read_to_string(file).map_err(|e| format!("{}: {}", file, e))?.trim().to_string(),
        None => String::new(),
    };
    let manifest = Manifest {
        version: version.trim_start_matches('v').to_string(),
        url: url.to_string(),
        notes,
        signature: String::new(),
    };
    let signature = key.sign(&signed_message(&manifest.version, &exe));
    let manifest = Manifest { signature: to_hex(&signature.to_bytes()), ..manifest };

    let public = to_hex(key.verifying_key().as_bytes());
    manifest::verify(&manifest, &exe, &public)?;
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(out, json).map_err(|e| format!("{}: {}", out, e))?;
    println!("{} {} allekirjoitettu (julkinen avain {})", exe_path, manifest.version, public);
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let need = |name| arg(args, name).ok_or(format!("{} puuttuu", name));
    let (exe_path, manifest_path, public) = (need("--exe")?, need("--manifest")?, need("--public-key")?);
    let exe = std::fs::read(exe_path).map_err(|e| format!("{}: {}", exe_path, e))?;
    let json = std::fs::read_to_string(manifest_path).map_err(|e| format!("{}: {}", manifest_path, e))?;
    let manifest: Manifest = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    manifest::verify(&manifest, &exe, public)?;
    println!("{} on aito versio {}", exe_path, manifest.version);
    Ok(())
}
