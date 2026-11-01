use crate::{utils::modules::states, wf_inventory::WarframeRootObject};
use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
type DecryptThingy = cbc::Decryptor<aes::Aes128>;
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use utils::*;

use serde::{Deserialize, Serialize};

use super::{helpers::*, traits::InventorySource};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WFInvAlecaframeSource {
    pub path: String,
    #[serde(skip, default = "default_stop_flag")]
    stop_flag: Arc<AtomicBool>,
}

impl PartialEq for WFInvAlecaframeSource {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

fn default_stop_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

const COMPONENT: &str = "WFInventory";

impl WFInvAlecaframeSource {
    pub fn get_default_path() -> PathBuf {
        crate::helper::get_local_data_path()
            .join("AlecaFrame")
            .join("lastData.dat")
    }
}

impl InventorySource for WFInvAlecaframeSource {
    fn update(&self, root: &Arc<Mutex<WarframeRootObject>>) -> Result<(), Error> {
        let path = if self.path.is_empty() {
            WFInvAlecaframeSource::get_default_path()
        } else {
            PathBuf::from(&self.path)
        };
        let parsed = block_on_async(load_inventory(&path))?;

        {
            let mut root = root.lock().map_err(|_| {
                Error::new(
                    "WFInvAlecaframeSource:Lock",
                    "Root mutex poisoned",
                    get_location!(),
                )
            })?;
            *root = parsed;
        }
        info(
            format!("{}:Update:Complete", COMPONENT),
            "Data file modified - root updated",
            &LoggerOptions::default(),
        );
        crate::send_event!(crate::types::UIEvent::RefreshWfInventory, serde_json::json!({}));
        Ok(())
    }

    fn start(&self, root: &Arc<Mutex<WarframeRootObject>>) {
        let source = self.clone();
        let root = root.clone();
        thread::spawn(move || {
            let path = if source.path.is_empty() {
                WFInvAlecaframeSource::get_default_path()
            } else {
                PathBuf::from(&source.path)
            };

            // Check if file exists before starting
            if !path.exists() {
                warning(
                    format!("{}:Watcher", COMPONENT),
                    format!("Inventory data file not found at: {}", path.display()),
                    &LoggerOptions::default(),
                );
            }

            let mut last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

            // Initial load if file exists
            if path.exists() {
                if let Err(e) = source.update(&root) {
                    e.log("WFInventoryState.log").with_location(get_location!());
                }
            }

            loop {
                thread::sleep(Duration::from_millis(500));

                if source.stop_flag.load(Ordering::Relaxed) {
                    info(
                        format!("{}:Watcher", COMPONENT),
                        "Watcher stopped",
                        &LoggerOptions::default(),
                    );
                    break;
                }

                match fs::metadata(&path).and_then(|m| m.modified()) {
                    Ok(modified) => {
                        if last_modified.map_or(true, |last| modified > last) {
                            last_modified = Some(modified);
                            if let Err(e) = source.update(&root) {
                                e.log("WFInventoryState.log").with_location(get_location!());
                            }
                        }
                    }
                    Err(_) => {
                        // File doesn't exist or can't be accessed - silently skip
                        // Reset last_modified so we catch it when it appears
                        if last_modified.is_some() {
                            last_modified = None;
                            warning(
                                format!("{}:Watcher", COMPONENT),
                                format!(
                                    "Inventory data file no longer accessible at: {}",
                                    path.display()
                                ),
                                &LoggerOptions::default(),
                            );
                        }
                    }
                }
            }
        });
    }

    fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn validate(&self) -> Result<(), Error> {
        let path = if self.path.is_empty() {
            WFInvAlecaframeSource::get_default_path()
        } else {
            PathBuf::from(&self.path)
        };

        if !path.exists() {
            return Err(Error::new(
                "WFInvAlecaframeSource:Validate",
                format!("Inventory data file not found at: {}", path.display()),
                get_location!(),
            ));
        }

        Ok(())
    }
}

/* ========================== */
/*        HELPERS             */
/* ========================== */

async fn load_inventory(path: &Path) -> Result<WarframeRootObject, Error> {
    let bytes = read_file(path)?;
    let data = decrypt_lastdata(&bytes).await?;
    let parsed = parse_lastdata(&data)?;
    Ok(parsed)
}

fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut file = File::open(path).map_err(|e| {
        Error::from_io(
            &format!("{COMPONENT}:Open"),
            &PathBuf::from(path),
            "Failed to open file",
            e,
            get_location!(),
        )
    })?;

    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| {
        Error::from_io(
            &format!("{COMPONENT}:Read"),
            &PathBuf::from(path),
            "Failed to read file",
            e,
            get_location!(),
        )
    })?;

    Ok(buf)
}

async fn decrypt_lastdata(data: &[u8]) -> Result<String, Error> {
    let af_api = states::app_state()?;

    let keys = match af_api.qf_client.alecaframe().get_decrypt_keys().await {
        Ok(keys) => keys,
        Err(err) => {
            return Err(Error::new(
                "DecryptLastData:GetKeys",
                format!("Failed to get decrypt keys: {err:?}"),
                get_location!(),
            ))
        }
    };

    let key: &[u8; 16] = keys.key.as_slice().try_into().map_err(|_| {
        Error::new(
            "DecryptLastData:KeySize",
            "Key must be 16 bytes",
            get_location!(),
        )
    })?;
    let iv: &[u8; 16] = keys.iv.as_slice().try_into().map_err(|_| {
        Error::new(
            "DecryptLastData:IvSize",
            "IV must be 16 bytes",
            get_location!(),
        )
    })?;

    let decrypted = DecryptThingy::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<NoPadding>(data)
        .map_err(|e| {
            Error::new(
                "DecryptLastData:Decrypt",
                format!("Decrypt failed: {e:?}"),
                get_location!(),
            )
        })?;

    String::from_utf8(decrypted).map_err(|e| {
        Error::new(
            "DecryptLastData:Utf8",
            format!("UTF-8 parse failed: {e:?}"),
            get_location!(),
        )
    })
}
