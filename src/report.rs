use anyhow::Result;
use crate::{PDError, VISUALIZATION_DATA};
use std::path::Path;
use log::{error, info};
use std::fs::File;
use std::io::Write;
use std::fs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use clap::Args;
use flate2::{Compression, write::GzEncoder, read::GzDecoder};

#[derive(Clone, Args, Debug)]
pub struct Report {
    /// Directory which contains run data to be visualized.
    #[clap(short, long, value_parser)]
    run_directory: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct API {
    name: String,
    runs: Vec<Run>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Run {
    name: String,
    keys: Vec<String>,
    key_values: HashMap<String, String>,
}

pub fn form_and_copy_archive(loc: String) -> Result<()> {
    if Path::new(&loc).is_dir() {
        let dir_stem = Path::new(&loc).file_stem().unwrap().to_str().unwrap().to_string();

        /* Create a temp archive */
        let archive_name = format!("{}.tar.gz", &dir_stem);
        let tar_gz = fs::File::create(&archive_name)?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.append_dir_all(&dir_stem, &loc)?;

        /* Copy archive to aperf_report */
        let archive_dst = format!("aperf_report/data/archive/{}", archive_name);
        fs::copy(&archive_name, archive_dst)?;
        return Ok(());
    }
    if infer::get_from_path(&loc)?.unwrap().mime_type() == "application/gzip" {
        let file_name = Path::new(&loc).file_name().unwrap().to_str().unwrap().to_string();

        /* Copy archive to aperf_report */
        let archive_dst = format!("aperf_report/data/archive/{}", file_name);
        fs::copy(loc, archive_dst)?;
        return Ok(());
    }
    return Err(PDError::RecordNotArchiveOrDirectory.into());
}

pub fn get_dir(dir: String) -> Result<String> {
    /* If dir return */
    if Path::new(&dir).is_dir() {
        return Ok(dir);
    }
    /* Unpack if archive */
    if infer::get_from_path(&dir)?.unwrap().mime_type() == "application/gzip" {
        let tar_gz = File::open(&dir)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(".")?;
        let dir_name = dir
            .strip_suffix(".tar.gz")
            .ok_or(PDError::InvalidArchiveName)?;
        if !Path::new(&dir_name).exists() {
            return Err(PDError::ArchiveDirectoryMismatch.into());
        }
        return Ok(dir_name.to_string());
    }
    return Err(PDError::RecordNotArchiveOrDirectory.into());
}

pub fn report(report: &Report) -> Result<()> {
    let dirs: Vec<String> = report.run_directory.clone();
    let mut dir_paths: Vec<String> = Vec::new();
    let mut dir_stems: Vec<String> = Vec::new();

    /* Get dir paths, stems */
    for dir in &dirs {
        let directory = get_dir(dir.to_string())?;
        let path = Path::new(&directory);
        if dir_stems.contains(&path.file_stem().unwrap().to_str().unwrap().to_string()) {
            error!("Cannot process two directories with the same name");
            return Ok(())
        }
        dir_stems.push(path.clone().file_stem().unwrap().to_str().unwrap().to_string());
        dir_paths.push(path.to_str().unwrap().to_string());
    }

    /* Generate report name */
    let mut report_name = "aperf_report".to_string();
    for name in &dir_stems {
        let file_name;
        if name.ends_with(".tar.gz") {
            file_name = name.strip_suffix(".tar.gz").unwrap().to_string();
        } else {
            file_name = name.to_string();
        }
        report_name = format!("{}_{}", report_name, file_name);
    }
    let report_name_tgz = format!("{}.tar.gz", report_name);

    /* Init visualizers */
    for dir in dir_paths {
        let name = VISUALIZATION_DATA.lock().unwrap().init_visualizers(dir.to_owned())?;
        VISUALIZATION_DATA.lock().unwrap().unpack_data(name)?;
    }

    info!("Creating APerf report...");
    let ico = include_bytes!("html_files/favicon.ico");
    let index_html = include_str!("html_files/index.html");
    let index_css = include_str!("html_files/index.css");
    let index_js = include_str!(concat!(env!("JS_DIR"), "/index.js"));
    let utils_js = include_str!(concat!(env!("JS_DIR"), "/utils.js"));
    let plotly_js = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/node_modules/plotly.js/dist/plotly.min.js"));
    let run_names = dir_stems.clone();

    fs::create_dir_all(report_name.clone() + "/js")?;
    fs::create_dir_all(report_name.clone() + "/data/archive")?;
    fs::create_dir_all(report_name.clone() + "/data/js")?;

    /* Generate/copy the archives of the collected data into aperf_report */
    for dir in &dirs {
        form_and_copy_archive(dir.clone())?;
    }
    /* Generate base HTML, JS files */
    let mut ico_file = File::create(report_name.clone() + "/favicon.ico")?;
    let mut index_html_file = File::create(report_name.clone() + "/index.html")?;
    let mut index_css_file = File::create(report_name.clone() + "/index.css")?;
    let mut index_js_file = File::create(report_name.clone() + "/index.js")?;
    let mut utils_js_file = File::create(report_name.clone() + "/js/utils.js")?;
    let mut plotly_js_file = File::create(report_name.clone() + "/js/plotly.js")?;
    ico_file.write_all(ico)?;
    write!(index_html_file, "{}", index_html)?;
    write!(index_css_file, "{}", index_css)?;
    write!(index_js_file, "{}", index_js)?;
    write!(utils_js_file, "{}", utils_js)?;
    write!(plotly_js_file, "{}", plotly_js)?;

    /* Generate visualizer JS files */
    for (name, file) in VISUALIZATION_DATA.lock().unwrap().get_all_js_files()? {
        let mut created_file = File::create(format!("{}/js/{}", report_name.clone(), name))?;
        write!(created_file, "{}", file)?;
    }

    /* Generate run.js */
    let out_loc = format!("{}/data/js/runs.js", report_name.clone());
    let mut runs_file = File::create(out_loc)?;
    write!(runs_file, "runs_raw = {}", serde_json::to_string(&run_names)?)?;
    let visualizer_names = VISUALIZATION_DATA.lock().unwrap().get_visualizer_names()?;

    /* Get visualizer data */
    for name in visualizer_names {
        let api_name = VISUALIZATION_DATA.lock().unwrap().get_api(name.clone())?;
        let calls = VISUALIZATION_DATA.lock().unwrap().get_calls(api_name.clone())?;
        let mut api = API {name: name.clone(), runs: Vec::new()};
        for run_name in &run_names {
            let mut temp_keys: Vec<String> = Vec::<String>::new();
            let mut run = Run {name: run_name.clone(), keys: Vec::new(), key_values: HashMap::new()};
            let mut keys = false;
            for call in &calls {
                let query = format!("run={}&get={}", run_name, call);
                let mut data;
                if call == "keys" {
                    data = VISUALIZATION_DATA.lock().unwrap().get_data(&api_name, query)?;
                    if data != "No data collected" {
                        temp_keys = serde_json::from_str(&data)?;
                    }
                    run.keys = temp_keys.clone();
                    keys = true;
                }
                if call == "values" {
                    if keys {
                        for key in &temp_keys {
                            let query = format!("run={}&get=values&key={}", run_name, key);
                            data = VISUALIZATION_DATA.lock().unwrap().get_data(&api_name, query)?;
                            run.key_values.insert(key.clone(), data.clone());
                        }
                    } else {
                        let query = format!("run={}&get=values", run_name);
                        data = VISUALIZATION_DATA.lock().unwrap().get_data(&api_name, query)?;
                        run.key_values.insert(call.clone(), data.clone());
                    }
                }
            }
            api.runs.push(run);
        }
        let out_loc = format!("{}/data/js/{}.js", report_name.clone(), api_name);
        let mut out_file = File::create(out_loc)?;
        let out_data = serde_json::to_string(&api)?;
        let str_out_data = format!("{}_raw_data = {}", api.name.clone(), out_data.clone());
        write!(out_file, "{}", str_out_data)?;
    }
    /* Generate aperf_report.tar.gz */
    info!("Generating {}", report_name_tgz);
    let tar_gz = File::create(&report_name_tgz)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(&report_name, &report_name)?;
    Ok(())
}
