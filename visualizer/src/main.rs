use csv::Reader;
use glob::glob;
use plotters::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::error::Error;
use std::fs;

const OUTPUT_DIR_STRONG: &str = "../output";
const OUTPUT_DIR_WEAK: &str = "../weak_scaling/output";

const STRONG_SCALING_DIFFICULTY: usize = 5;
const SEQUENTIAL_PORTION: f64 = 0.02;

#[derive(Debug, Deserialize)]
struct PerformanceRecord {
    total_time_seconds: f64,
}

#[derive(Debug, Clone)]
struct RunStats {
    workers: usize,
    mean_time: f64,
    std_dev: f64,
    times: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct PosValidationRecord {
    validator_id: usize,
    block_index: usize,
    success: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    fs::create_dir_all("grafici")?;

    let mode = if args.len() > 1 {
        args[1].as_str()
    } else {
        "scaling"
    };

    match mode {
        "pos" => run_pos_visualization()?,
        "scaling" => run_scaling_analysis()?,
        _ => {
            println!("Upotreba: cargo run [pos|scaling]");
            println!("  pos     - Vizualizacija PoS validacije");
            println!("  scaling - Analiza skaliranja (default)");
        }
    }

    Ok(())
}

fn run_scaling_analysis() -> Result<(), Box<dyn Error>> {
    println!("--- Analiza Performansi PoW Implementacija ---");
    println!("Konfiguracija:");
    println!("  - Direktorijum za jako skaliranje: {}", OUTPUT_DIR_STRONG);
    println!("  - Direktorijum za slabo skaliranje: {}", OUTPUT_DIR_WEAK);
    println!(
        "  - Tezina za jako skaliranje: d={}",
        STRONG_SCALING_DIFFICULTY
    );
    println!(
        "  - Procenjeni sekvencijalni deo (s): {:.2}%",
        SEQUENTIAL_PORTION * 100.0
    );

    println!("\n[1] Analiza jakog skaliranja...");
    analyze_strong_scaling("Rust")?;
    analyze_strong_scaling("Python")?;

    println!("\n[2] Analiza slabog skaliranja...");
    analyze_weak_scaling("Rust")?;
    analyze_weak_scaling("Python")?;

    println!("\nAnaliza zavrsena. Grafici su sacuvani.");
    Ok(())
}

fn analyze_strong_scaling(lang: &str) -> Result<(), Box<dyn Error>> {
    let stats = load_and_aggregate_data(lang, Some(STRONG_SCALING_DIFFICULTY), OUTPUT_DIR_STRONG)?;

    if stats.is_empty() {
        println!("  -  Nisu pronadjeni podaci za jako skaliranje ({}).", lang);
        return Ok(());
    }

    println!("  - Rezultati za {}:", lang);
    print_stats_table(&stats);

    let output_file = format!("grafici/jako_skaliranje_{}.png", lang.to_lowercase());
    let title = format!("Jako skaliranje - {}", lang);
    create_strong_scaling_plot(&output_file, &title, &stats)?;
    println!("    - Grafik sacuvan: {}", output_file);
    Ok(())
}

fn analyze_weak_scaling(lang: &str) -> Result<(), Box<dyn Error>> {
    let stats = load_and_aggregate_data(lang, None, OUTPUT_DIR_WEAK)?;

    if stats.is_empty() {
        println!("  - Nisu pronadjeni podaci za slabo skaliranje ({}).", lang);
        return Ok(());
    }

    println!("  - Rezultati za {}:", lang);
    print_stats_table(&stats);

    let output_file = format!("grafici/slabo_skaliranje_{}.png", lang.to_lowercase());
    let title = format!("Slabo skaliranje - {}", lang);
    create_weak_scaling_plot(&output_file, &title, &stats)?;
    println!("    - Grafik sacuvan: {}", output_file);
    Ok(())
}

fn load_and_aggregate_data(
    lang: &str,
    fixed_difficulty: Option<usize>,
    output_dir: &str,
) -> Result<Vec<RunStats>, Box<dyn Error>> {
    let pattern = match fixed_difficulty {
        Some(d) => format!(
            "{}/pow_performance_*_{}_d{}_*.csv",
            output_dir,
            lang.to_lowercase(),
            d
        ),
        None => format!(
            "{}/pow_performance_*_{}_*.csv",
            output_dir,
            lang.to_lowercase()
        ),
    };

    let mut grouped_times: BTreeMap<usize, Vec<f64>> = BTreeMap::new();

    for entry in glob(&pattern)? {
        let path = entry?;
        let filename = path.file_name().unwrap().to_str().unwrap();

        let workers = filename
            .split('_')
            .find(|p| p.starts_with('w'))
            .and_then(|p| p[1..].parse::<usize>().ok())
            .unwrap_or(1);

        let mut rdr = Reader::from_path(&path)?;
        for result in rdr.deserialize::<PerformanceRecord>() {
            grouped_times
                .entry(workers)
                .or_default()
                .push(result?.total_time_seconds);
        }
    }

    let mut final_stats = Vec::new();
    for (workers, times) in grouped_times {
        if times.is_empty() {
            continue;
        }
        let mean = calculate_mean(&times);
        let std_dev = calculate_std_dev(&times, mean);
        final_stats.push(RunStats {
            workers,
            mean_time: mean,
            std_dev,
            times,
        });
    }
    Ok(final_stats)
}

fn print_stats_table(stats: &[RunStats]) {
    println!("    | Radnika | Br. Merenja | Srednje Vreme (s) | Std. Dev. (s) |");
    println!("    |---------|-------------|-------------------|---------------|");
    for s in stats {
        println!(
            "    | {:>7} | {:>11} | {:>17.4} | {:>13.4} |",
            s.workers,
            s.times.len(),
            s.mean_time,
            s.std_dev
        );
    }
}

fn calculate_mean(data: &[f64]) -> f64 {
    data.iter().sum::<f64>() / data.len() as f64
}

fn calculate_std_dev(data: &[f64], mean: f64) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let variance = data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

fn create_strong_scaling_plot(
    output_path: &str,
    title: &str,
    stats: &[RunStats],
) -> Result<(), Box<dyn Error>> {
    let root = BitMapBackend::new(output_path, (1024, 768)).into_drawing_area();
    root.fill(&WHITE)?;

    let sequential_mean_time = stats
        .iter()
        .find(|s| s.workers == 1)
        .map_or(1.0, |s| s.mean_time);
    let max_workers = stats.iter().map(|s| s.workers).max().unwrap_or(1);
    let max_speedup = stats
        .iter()
        .map(|s| sequential_mean_time / s.mean_time)
        .fold(0.0, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 50))
        .margin(15)
        .x_label_area_size(50)
        .y_label_area_size(60)
        .build_cartesian_2d(0..max_workers + 1, 0.0..max_speedup * 1.2)?;

    chart
        .configure_mesh()
        .x_desc("Broj radnika (p)")
        .y_desc("Ubrzanje S(p)")
        .draw()?;

    chart.draw_series(stats.iter().map(|s| {
        let speedup = sequential_mean_time / s.mean_time;
        let speedup_std_dev = (s.std_dev / s.mean_time) * speedup;
        ErrorBar::new_vertical(
            s.workers,
            speedup - speedup_std_dev,
            speedup,
            speedup + speedup_std_dev,
            BLUE.filled(),
            10,
        )
    }))?;

    chart
        .draw_series(LineSeries::new(
            stats
                .iter()
                .map(|s| (s.workers, sequential_mean_time / s.mean_time)),
            &BLUE,
        ))?
        .label("Eksperimentalno ubrzanje")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    let amdahl_law = |p: usize| 1.0 / (SEQUENTIAL_PORTION + (1.0 - SEQUENTIAL_PORTION) / p as f64);
    chart
        .draw_series(LineSeries::new(
            (1..=max_workers).map(|p| (p, amdahl_law(p))),
            &RED,
        ))?
        .label(format!(
            "Amdalov Zakon (s={:.0}%)",
            SEQUENTIAL_PORTION * 100.0
        ))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart
        .draw_series(LineSeries::new(
            (0..=max_workers).map(|p| (p, p as f64)),
            (&BLACK).stroke_width(2),
        ))?
        .label("Idealno ubrzanje")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLACK));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;
    root.present()?;
    Ok(())
}

fn run_pos_visualization() -> Result<(), Box<dyn Error>> {
    let pattern = "../rust/output/pos_validation_rust_*.json";
    let json_files: Vec<_> = glob(pattern)?.collect();

    if json_files.is_empty() {
        println!("Nije pronadjen nijedan pos_validation_rust_*.json fajl");
        return Ok(());
    }

    let json_path = json_files[0].as_ref().unwrap();
    println!("Ucitavam: {:?}", json_path);

    let json_data = fs::read_to_string(json_path)?;
    let records: Vec<PosValidationRecord> = serde_json::from_str(&json_data)?;

    let mut blocks: BTreeMap<usize, Vec<&PosValidationRecord>> = BTreeMap::new();
    for record in &records {
        blocks.entry(record.block_index).or_default().push(record);
    }

    let mut successful_validators: BTreeMap<usize, usize> = BTreeMap::new();
    let mut validator_selection_count: HashMap<usize, usize> = HashMap::new();

    for (block_idx, block_records) in &blocks {
        for record in block_records {
            if record.success {
                successful_validators.insert(*block_idx, record.validator_id);
                *validator_selection_count
                    .entry(record.validator_id)
                    .or_insert(0) += 1;
                break;
            }
        }
    }

    let total_validators = records.iter().map(|r| r.validator_id).max().unwrap_or(0) + 1;

    create_pos_animation(
        "grafici/pos_validation.gif",
        &successful_validators,
        &validator_selection_count,
        total_validators,
    )?;

    println!("Animacija sacuvana: grafici/pos_validation.gif");
    Ok(())
}

fn create_pos_animation(
    output_path: &str,
    successful_validators: &BTreeMap<usize, usize>,
    _validator_counts: &HashMap<usize, usize>,
    total_validators: usize,
) -> Result<(), Box<dyn Error>> {
    use gif::{Encoder, Frame, Repeat};
    use plotters::prelude::*;
    use std::fs::File;

    let width = 1000;
    let height = 600;
    let frame_delay = 30;

    fs::create_dir_all("grafici/temp_frames")?;

    let mut frame_paths = Vec::new();

    let val_box_size = 60_usize;
    let val_spacing = 10_usize;
    let validators_per_row = (width as usize - 40) / (val_box_size + val_spacing);

    let block_box_size = 80_usize;
    let block_spacing = 10_usize;
    let blocks_per_row = (width as usize - 40) / (block_box_size + block_spacing);

    let mut current_validator_counts: HashMap<usize, usize> = HashMap::new();
    let mut blocks_added: Vec<(usize, usize)> = Vec::new();

    for (frame_idx, (block_index, validator_id)) in successful_validators.iter().enumerate() {
        *current_validator_counts.entry(*validator_id).or_insert(0) += 1;
        blocks_added.push((*block_index, *validator_id));

        let frame_path = format!("grafici/temp_frames/frame_{:03}.png", frame_idx);
        frame_paths.push(frame_path.clone());

        let root =
            BitMapBackend::new(&frame_path, (width as u32, height as u32)).into_drawing_area();
        root.fill(&WHITE)?;
        let (validator_area, block_area) = root.split_vertically(height / 2);

        validator_area.fill(&RGBColor(245, 245, 250))?;
        validator_area.draw(&Text::new(
            format!("Validatori - Blok {} dodat", block_index),
            (20, 20),
            ("sans-serif", 30).into_font().color(&BLACK),
        ))?;

        for vid in 0..total_validators {
            let row = vid / validators_per_row;
            let col = vid % validators_per_row;
            let x = 20 + col * (val_box_size + val_spacing);
            let y = 60 + row * (val_box_size + val_spacing);
            let count = current_validator_counts.get(&vid).unwrap_or(&0);

            let color = if vid == *validator_id {
                RGBColor(124, 252, 0)
            } else {
                RGBColor(255, 255, 255)
            };

            validator_area.draw(&Rectangle::new(
                [
                    (x as i32, y as i32),
                    ((x + val_box_size) as i32, (y + val_box_size) as i32),
                ],
                color.filled(),
            ))?;

            validator_area.draw(&Rectangle::new(
                [
                    (x as i32, y as i32),
                    ((x + val_box_size) as i32, (y + val_box_size) as i32),
                ],
                BLACK.stroke_width(2),
            ))?;

            let text_color = if vid == *validator_id { WHITE } else { BLACK };

            validator_area.draw(&Text::new(
                format!("V{}", vid),
                (x as i32 + 5, y as i32 + 10),
                ("sans-serif", 14).into_font().color(&text_color),
            ))?;

            if *count > 0 {
                validator_area.draw(&Text::new(
                    format!("{}", count),
                    (x as i32 + 20, y as i32 + 35),
                    ("sans-serif", 18).into_font().color(&text_color),
                ))?;
            }
        }

        block_area.fill(&RGBColor(250, 245, 245))?;
        block_area.draw(&Text::new(
            "Blockchain",
            (20, 20),
            ("sans-serif", 30).into_font().color(&BLACK),
        ))?;

        for (idx, (bidx, vid)) in blocks_added.iter().enumerate() {
            let row = idx / blocks_per_row;
            let col = idx % blocks_per_row;
            let x = 20 + col * (block_box_size + block_spacing);
            let y = 60 + row * (block_box_size + block_spacing);

            let color = if bidx == block_index {
                RGBColor(150, 200, 255)
            } else {
                RGBColor(100, 150, 255)
            };

            block_area.draw(&Rectangle::new(
                [
                    (x as i32, y as i32),
                    ((x + block_box_size) as i32, (y + block_box_size) as i32),
                ],
                color.filled(),
            ))?;

            block_area.draw(&Rectangle::new(
                [
                    (x as i32, y as i32),
                    ((x + block_box_size) as i32, (y + block_box_size) as i32),
                ],
                BLACK.stroke_width(3),
            ))?;

            block_area.draw(&Text::new(
                format!("B{}", bidx),
                (x as i32 + 5, y as i32 + 10),
                ("sans-serif", 16).into_font().color(&WHITE),
            ))?;

            block_area.draw(&Text::new(
                format!("V{}", vid),
                (x as i32 + 20, y as i32 + 45),
                ("sans-serif", 14).into_font().color(&WHITE),
            ))?;
        }

        root.present()?;
    }

    let mut gif_file = File::create(output_path)?;
    let mut encoder = Encoder::new(&mut gif_file, width as u16, height as u16, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;

    for frame_path in &frame_paths {
        let img = image::open(frame_path)?.to_rgba8();
        let mut frame =
            Frame::from_rgba_speed(width as u16, height as u16, &mut img.into_raw(), 10);
        frame.delay = frame_delay as u16;
        encoder.write_frame(&frame)?;
    }

    drop(encoder);
    drop(gif_file);

    for frame_path in &frame_paths {
        let _ = fs::remove_file(frame_path);
    }
    let _ = fs::remove_dir("grafici/temp_frames");

    Ok(())
}

fn create_weak_scaling_plot(
    output_path: &str,
    title: &str,
    stats: &[RunStats],
) -> Result<(), Box<dyn Error>> {
    if stats.is_empty() {
        return Ok(());
    }

    let root = BitMapBackend::new(output_path, (1024, 768)).into_drawing_area();
    root.fill(&WHITE)?;

    let base_case = stats
        .iter()
        .min_by_key(|s| s.workers)
        .ok_or("Nije pronadjen nijedan podatak za slabo skaliranje")?;
    let t_base = base_case.mean_time;

    let max_workers = stats.iter().map(|s| s.workers).max().unwrap_or(1);

    let max_scaled_speedup = stats
        .iter()
        .map(|s| s.workers as f64 * (t_base / s.mean_time))
        .fold(0.0, f64::max);

    let gustafson_max = max_workers as f64 - (max_workers as f64 - 1.0) * SEQUENTIAL_PORTION;

    let y_max = f64::max(max_scaled_speedup, gustafson_max).max(max_workers as f64) * 1.1;

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 50))
        .margin(15)
        .x_label_area_size(50)
        .y_label_area_size(60)
        .build_cartesian_2d(0..max_workers + 1, 0.0..y_max)?;

    chart
        .configure_mesh()
        .x_desc("Broj radnika (p)")
        .y_desc("Scaled speedup")
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            stats.iter().map(|s| {
                let scaled_speedup = s.workers as f64 * (t_base / s.mean_time);
                (s.workers, scaled_speedup)
            }),
            &BLUE,
        ))?
        .label("Eksperimentalni scaled speedup")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    chart
        .draw_series(LineSeries::new(
            (0..=max_workers).map(|p| (p, p as f64)),
            (&BLACK).stroke_width(2),
        ))?
        .label("Idealni scaled speedup")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLACK));

    let gustafson_law = |p: usize| p as f64 - (p as f64 - 1.0) * SEQUENTIAL_PORTION;
    chart
        .draw_series(LineSeries::new(
            (1..=max_workers).map(|p| (p, gustafson_law(p))),
            &RED,
        ))?
        .label(format!(
            "Gustafsonov zakon (s={:.0}%)",
            SEQUENTIAL_PORTION * 100.0
        ))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;
    root.present()?;
    Ok(())
}
