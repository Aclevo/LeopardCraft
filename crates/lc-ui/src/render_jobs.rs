//! Background render job queue, polling, start and save workflows.

use super::*;

impl LeopardApp {
    pub(super) fn push_render_job(&mut self, job: RenderJob) {
        let id = job.id;
        self.render_jobs.push(job);
        while self.render_jobs.len() > 8 {
            let removed = self.render_jobs.remove(0);
            if self.selected_render_job == Some(removed.id) {
                self.selected_render_job = None;
            }
        }
        self.selected_render_job = Some(id);
    }

    pub(super) fn selected_render(&self) -> Option<&RenderJob> {
        self.selected_render_job
            .and_then(|id| self.render_jobs.iter().find(|job| job.id == id))
    }

    pub(super) fn selected_finished_render(&self) -> Option<&RenderJob> {
        self.selected_render().filter(|job| job.is_finished())
    }

    pub(super) fn poll_render(&mut self, ctx: &egui::Context) {
        let polled = self
            .active_render
            .as_ref()
            .map(|active| active.rx.try_recv());
        match polled {
            None => {}
            Some(Ok(RenderMessage::Progress { done, total })) => {
                if let Some(active) = self.active_render.as_mut() {
                    active.done = done;
                    active.total = total;
                }
                ctx.request_repaint_after(Duration::from_millis(120));
            }
            Some(Ok(RenderMessage::Done(result))) => {
                let Some(active) = self.active_render.take() else {
                    return;
                };
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [result.width as usize, result.height as usize],
                    &result.rgba,
                );
                let texture = ctx.load_texture(
                    format!("render-job-{}", active.id),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                let job_id = active.id;
                self.push_render_job(RenderJob {
                    id: job_id,
                    name: active.name,
                    width: result.width,
                    height: result.height,
                    samples: active.total,
                    samples_done: active.total,
                    seconds: active.started.elapsed().as_secs_f32(),
                    exposure: active.exposure,
                    date: utc_now_string(),
                    file: active.file,
                    status: RenderJobStatus::Finished,
                    texture: Some(texture),
                    pixels: Some(result.pixels),
                });
                self.status = format!(
                    "Rendered {}×{} ({} spp)",
                    result.width, result.height, active.total
                );
                self.show_render_manager = true;
            }
            Some(Ok(RenderMessage::Failed(error))) => {
                self.active_render = None;
                self.status = format!("Render failed: {error}");
            }
            Some(Ok(RenderMessage::Cancelled)) => {
                if let Some(active) = self.active_render.take() {
                    self.push_render_job(cancelled_render_job(&active));
                }
                self.status = "Render cancelled".into();
                self.show_render_manager = true;
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.active_render = None;
                self.status = "Render worker stopped unexpectedly".into();
            }
        }
    }

    pub(super) fn start_render(&mut self, ctx: &egui::Context) {
        if self.active_render.is_some() {
            self.status = "A render is already running".into();
            self.show_render_manager = true;
            return;
        }
        let doc = self.doc.clone();
        let time = self.playback.time;
        let settings = self.doc.settings.render.clone();
        let base_dir = self
            .lifecycle
            .path()
            .and_then(|path| path.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let output = if self.render_path.trim().is_empty() {
            "render.png".to_string()
        } else {
            self.render_path.clone()
        };
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let exposure = settings.exposure;
        std::thread::spawn(move || {
            let result = (|| -> Result<RenderResult, String> {
                let scene = RenderScene::from_document(&doc, time, base_dir)?;
                let mut tracer = PathTracer::new(settings.width, settings.height, settings.seed);
                let mut done = 0u32;
                while done < settings.samples {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Err("__cancelled__".into());
                    }
                    let chunk = (settings.samples - done).min(4);
                    tracer.render_pass(&scene, chunk);
                    done += chunk;
                    let _ = tx.send(RenderMessage::Progress {
                        done,
                        total: settings.samples,
                    });
                }
                let pixels = tracer.hdr_pixels();
                let rgba = tracer
                    .rgba8(settings.exposure, settings.transparent_background)
                    .into_iter()
                    .flatten()
                    .collect();
                Ok(RenderResult {
                    width: settings.width,
                    height: settings.height,
                    pixels,
                    rgba,
                })
            })();
            match result {
                Ok(result) => {
                    let _ = tx.send(RenderMessage::Done(Box::new(result)));
                }
                Err(error) if error == "__cancelled__" => {
                    let _ = tx.send(RenderMessage::Cancelled);
                }
                Err(error) => {
                    let _ = tx.send(RenderMessage::Failed(error));
                }
            }
        });
        let name = Path::new(&output)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Untitled")
            .to_string();
        self.active_render = Some(ActiveRender {
            rx,
            cancel,
            id: self.next_job_id,
            name,
            file: output,
            width: settings.width,
            height: settings.height,
            total: settings.samples,
            done: 0,
            exposure,
            started: Instant::now(),
        });
        self.next_job_id += 1;
        self.show_render_manager = true;
        self.status = "Rendering on CPU…".into();
        ctx.request_repaint();
    }

    pub(super) fn save_render(&mut self, exr: bool) {
        let Some(id) = self.selected_render_job else {
            self.status = "Render a frame before saving".into();
            return;
        };
        let outcome = {
            let Some(job) = self.render_jobs.iter().find(|job| job.id == id) else {
                self.status = "Select a finished render job".into();
                return;
            };
            if !job.is_finished() {
                self.status = "Select a finished render job".into();
                return;
            }
            let Some(pixels) = job.pixels.as_ref() else {
                self.status = "Select a finished render job".into();
                return;
            };
            let mut path = PathBuf::from(if job.file.trim().is_empty() {
                self.render_path.trim()
            } else {
                job.file.trim()
            });
            if path.as_os_str().is_empty() {
                path = PathBuf::from("render.png");
            }
            if exr {
                path.set_extension("exr");
                lc_render::save_linear_exr(&path, pixels, job.width, job.height)
                    .map(|()| path.clone())
                    .map_err(|e| format!("EXR save failed: {e}"))
            } else {
                path.set_extension("png");
                lc_render::save_linear_png(&path, pixels, job.width, job.height, job.exposure)
                    .map(|()| path.clone())
                    .map_err(|e| format!("PNG save failed: {e}"))
            }
        };
        self.status = match outcome {
            Ok(path) => format!("Saved {}", path.display()),
            Err(error) => error,
        };
    }
}
