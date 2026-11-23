use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::PathBuf;

pub fn local_binary() -> PathBuf {
    crate::job::data_dir()
        .join("ffmpeg")
        .join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        })
}

fn url() -> &'static str {
    if cfg!(windows) {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
    } else {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz"
    }
}

pub enum Stage {
    Downloading(u32),
    Extracting,
}

pub fn install(progress: &dyn Fn(Stage)) -> Result<PathBuf> {
    let tmp = crate::job::data_dir().join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join(if cfg!(windows) {
        "ffmpeg.zip"
    } else {
        "ffmpeg.tar.xz"
    });
    let response = ureq::get(url())
        .call()
        .context("could not download ffmpeg")?;
    let total: u64 = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&archive)?;
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    let mut last = u32::MAX;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        let pct = if total > 0 {
            (done * 100 / total) as u32
        } else {
            0
        };
        if pct != last {
            last = pct;
            progress(Stage::Downloading(pct));
        }
    }
    drop(file);
    progress(Stage::Extracting);
    let target = local_binary();
    std::fs::create_dir_all(target.parent().unwrap())?;
    let partial = target.with_extension("partial");
    let mut found = false;
    if cfg!(windows) {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive)?)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.name().replace('\\', "/").ends_with("/bin/ffmpeg.exe") {
                let mut out = std::fs::File::create(&partial)?;
                std::io::copy(&mut entry, &mut out)?;
                found = true;
                break;
            }
        }
    } else {
        let tar_path = tmp.join("ffmpeg.tar");
        {
            let mut input = std::io::BufReader::new(std::fs::File::open(&archive)?);
            let mut output = std::io::BufWriter::new(std::fs::File::create(&tar_path)?);
            lzma_rs::xz_decompress(&mut input, &mut output)
                .map_err(|e| anyhow::anyhow!("xz: {e:?}"))?;
        }
        let mut tar = tar::Archive::new(std::fs::File::open(&tar_path)?);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().replace('\\', "/");
            if path.ends_with("/bin/ffmpeg") {
                let mut out = std::fs::File::create(&partial)?;
                std::io::copy(&mut entry, &mut out)?;
                found = true;
                break;
            }
        }
        let _ = std::fs::remove_file(&tar_path);
    }
    let _ = std::fs::remove_file(&archive);
    if !found {
        bail!("ffmpeg was not found inside the download");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
    }
    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }
    std::fs::rename(&partial, &target)?;
    Ok(target)
}
