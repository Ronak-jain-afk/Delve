pub struct Progress {
    spinner: Option<indicatif::ProgressBar>,
}

impl Progress {
    pub fn new(enabled: bool) -> Self {
        if enabled {
            let spinner = indicatif::ProgressBar::new_spinner();
            spinner.set_style(
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .expect("valid template"),
            );
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));
            Progress { spinner: Some(spinner) }
        } else {
            Progress { spinner: None }
        }
    }

    pub fn set_message(&self, msg: &str) {
        if let Some(ref spinner) = self.spinner {
            spinner.set_message(msg.to_string());
        }
    }

    pub fn finish(&self) {
        if let Some(ref spinner) = self.spinner {
            spinner.finish_and_clear();
        }
    }
}
