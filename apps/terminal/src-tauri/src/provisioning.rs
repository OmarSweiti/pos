//! First-run custody (microstep 1.8.5b, its second part): a machine with no
//! register yet, and the recovery code a new one is given.
//!
//! **A clean machine creates nothing.** Before any key exists, first run asks
//! for a choice between a **new register** and **restoring an existing one**
//! (`ref/security-compliance.md` §6a). The restore path, 1.8.7's, must find no
//! fresh key and no empty database standing where the old register's belong,
//! so a machine with no database opens [`crate::state::Till::Unprovisioned`]
//! and writes nothing at all until a person chooses.
//!
//! **A new register's custody, in the order a crash can survive:**
//! [`provision_recovery_code`] takes the key from the credential store, minting
//! one only when there is none and no database ([`pos_db::key`]); wraps it under
//! a fresh code into a *pending* envelope; creates the empty database; and
//! shows the code, once. The code is held in memory only. A crash at any point
//! leaves a machine that comes back to this same choice, or to a fresh code:
//!
//! - before the database exists, it is still a new machine, and the key the
//!   store kept is the one used again;
//! - after it, the pending envelope says the code was never confirmed, and the
//!   next code is wrapped over it, so the unconfirmed one opens nothing.
//!
//! [`acknowledge_recovery_code`] is the merchant saying the code is recorded.
//! The pending envelope must open with the code held, to this register's key,
//! before it becomes the register's envelope; then the code is dropped from
//! memory, wiped.
//!
//! **The register's identity is 1.8.5c's,** by the owner's decision of 12
//! November 2026: the business, the store, the register row, the device id and
//! the first owner. Until it lands, a machine whose custody is complete waits
//! at [`SetupStage::StoreNotSetUp`], and demo registers come from the seed.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pos_db::key::{self, CredentialStore, DatabaseKey, OsCredentialStore};
use pos_db::recovery::{
    self as custody, ENVELOPE_FILE, KdfParams, PENDING_ENVELOPE_FILE, RecoveryCode,
};
use serde::{Serialize, Serializer};
use tauri::State;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::ipc::{IpcError, IpcResult};
use crate::state::{DATABASE_FILE, DEVICE_ID_FILE, Till};

/// Where the register's database key is kept, and the dev/CI key this build
/// honours (1.8.5): read once, at start-up.
#[derive(Clone)]
pub struct KeyCustody {
    /// `POS_DB_KEY`, in a debug build that has it set; `None` otherwise.
    pub env: Option<String>,
    pub store: Arc<dyn CredentialStore + Send + Sync>,
}

impl KeyCustody {
    /// The OS credential store, and `POS_DB_KEY` where this build honours it.
    pub fn os() -> Self {
        Self {
            env: key::env_key(),
            store: Arc::new(OsCredentialStore),
        }
    }

    /// `store`, with no environment key: what a test states.
    pub fn in_store(store: Arc<dyn CredentialStore + Send + Sync>) -> Self {
        Self { env: None, store }
    }

    /// The key of a database that exists. Never a new one.
    ///
    /// # Errors
    ///
    /// As [`pos_db::key::get_existing_with`].
    pub fn existing(&self) -> Result<DatabaseKey, pos_db::DbError> {
        key::get_existing_with(self.env.clone(), self.store.as_ref()).map(|(key, _)| key)
    }
}

/// How far a machine with no open register has come, as `session_state` and
/// every refused command say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStage {
    /// No database: nothing has been created. A person chooses a new register
    /// or a restore.
    NewMachine,
    /// A code was shown and never confirmed. The next one replaces it.
    CodeNotConfirmed,
    /// Custody is complete; the register's store is not set up (1.8.5c).
    StoreNotSetUp,
}

impl SetupStage {
    /// The stage, from a fixed set, as the screen is told it.
    pub fn detail(self) -> &'static str {
        match self {
            SetupStage::NewMachine => "this machine holds no register yet",
            SetupStage::CodeNotConfirmed => "the recovery code was never confirmed",
            SetupStage::StoreNotSetUp => "the register's store is not set up yet",
        }
    }
}

/// The stage of the machine whose data is in `data_dir`, read from the files
/// beside its database, or `None` when it holds a register to open.
///
/// An answer the file system cannot give is `None`: opening never mints a
/// key, so the refusal it meets says what is wrong, and nothing is created.
pub fn stage_of(data_dir: &Path) -> Option<SetupStage> {
    let here = |name: &str| data_dir.join(name).try_exists().ok();
    if !here(DATABASE_FILE)? {
        return Some(SetupStage::NewMachine);
    }
    let pending = here(PENDING_ENVELOPE_FILE)?;
    let confirmed = here(ENVELOPE_FILE)?;
    if pending && !confirmed {
        return Some(SetupStage::CodeNotConfirmed);
    }
    if confirmed && !here(DEVICE_ID_FILE)? {
        return Some(SetupStage::StoreNotSetUp);
    }
    None
}

/// What `provision_recovery_code` shows: the code, once, and the id its
/// confirmation names. It never prints itself.
#[derive(Serialize)]
pub struct RecoveryCodeDisplay {
    pub provisioning_id: Uuid,
    /// Six groups of five digits, joined by hyphens. Wiped when dropped.
    #[serde(serialize_with = "plain")]
    pub formatted_code: Zeroizing<String>,
}

fn plain<S: Serializer>(code: &Zeroizing<String>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(code)
}

impl std::fmt::Debug for RecoveryCodeDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryCodeDisplay")
            .field("provisioning_id", &self.provisioning_id)
            .field("formatted_code", &"redacted")
            .finish()
    }
}

/// Where a machine with no register is, held for the life of the process.
enum Custody {
    /// Waiting for a person: a new machine, a code never confirmed, or a store
    /// not set up.
    Waiting(SetupStage),
    /// A code is on the screen. It is held here, and nowhere else, until it
    /// is confirmed; then it is dropped and wiped.
    Shown {
        provisioning_id: Uuid,
        code: RecoveryCode,
    },
}

/// A machine with no open register: its data directory, its key's custody,
/// and how far setting it up has come.
pub struct Provisioning {
    data_dir: PathBuf,
    keys: KeyCustody,
    custody: Mutex<Custody>,
}

impl Provisioning {
    /// The machine in `data_dir`, at `stage`.
    pub fn new(data_dir: &Path, keys: KeyCustody, stage: SetupStage) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            keys,
            custody: Mutex::new(Custody::Waiting(stage)),
        }
    }

    /// How far setting up has come. A code on the screen is one not yet
    /// confirmed.
    pub fn stage(&self) -> SetupStage {
        match &*self.lock() {
            Custody::Waiting(stage) => *stage,
            Custody::Shown { .. } => SetupStage::CodeNotConfirmed,
        }
    }

    /// What every command that needs a register answers here.
    pub fn refusal(&self) -> IpcError {
        IpcError::new("not_set_up", "error.till.not_set_up").with_detail(self.stage().detail())
    }

    fn lock(&self) -> MutexGuard<'_, Custody> {
        self.custody.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `provision_recovery_code`: a new register's key, its empty database,
    /// and its recovery code, shown once. See the module's documentation for
    /// the order, and what a crash leaves.
    ///
    /// # Errors
    ///
    /// `recovery_code_already_shown` while a code is on the screen;
    /// `already_set_up` once custody is complete; `provisioning_failed`, with
    /// a fixed detail, when the key, the envelope or the database could not be
    /// made or kept.
    pub fn issue(&self) -> IpcResult<RecoveryCodeDisplay> {
        let mut custody = self.lock();
        let stage = match &*custody {
            Custody::Shown { .. } => {
                return Err(IpcError::new(
                    "recovery_code_already_shown",
                    "error.provisioning.already_shown",
                ));
            }
            Custody::Waiting(SetupStage::StoreNotSetUp) => {
                return Err(already_set_up());
            }
            Custody::Waiting(stage) => *stage,
        };
        let database = self.data_dir.join(DATABASE_FILE);
        let key = match stage {
            SetupStage::NewMachine => {
                key::get_or_create_with(self.keys.env.clone(), self.keys.store.as_ref(), &database)
                    .map(|(key, _)| key)
            }
            _ => self.keys.existing(),
        }
        .map_err(|error| failed("the database key could not be had", &error))?;
        let code = RecoveryCode::generate()
            .map_err(|error| failed("no recovery code was issued", &error))?;
        KdfParams::fresh()
            .and_then(|params| custody::wrap_data_key(&key, &code, &params))
            .and_then(|envelope| envelope.write(&self.data_dir.join(PENDING_ENVELOPE_FILE)))
            .map_err(|error| failed("the recovery envelope could not be written", &error))?;
        if stage == SetupStage::NewMachine {
            pos_db::open(&database, key.expose())
                .map_err(|error| failed("the register database could not be created", &error))?;
        }
        let provisioning_id = random_id()?;
        let display = RecoveryCodeDisplay {
            provisioning_id,
            formatted_code: code.formatted(),
        };
        *custody = Custody::Shown {
            provisioning_id,
            code,
        };
        Ok(display)
    }

    /// `acknowledge_recovery_code`: the merchant says the code shown under
    /// `provisioning_id` is recorded. Its pending envelope must open with it,
    /// to this register's key, before it becomes the register's envelope;
    /// then the code is dropped from memory, wiped.
    ///
    /// # Errors
    ///
    /// `recovery_code_not_shown` when no code is on the screen;
    /// `provisioning_id_unknown` for another id; `provisioning_failed` when
    /// the key cannot be read or the envelope does not confirm. A refusal
    /// keeps the code on the screen.
    pub fn confirm(&self, provisioning_id: Uuid) -> IpcResult<()> {
        let mut custody = self.lock();
        let Custody::Shown {
            provisioning_id: shown,
            code,
        } = &*custody
        else {
            return Err(IpcError::new(
                "recovery_code_not_shown",
                "error.provisioning.not_shown",
            ));
        };
        if *shown != provisioning_id {
            return Err(IpcError::new(
                "provisioning_id_unknown",
                "error.provisioning.id_unknown",
            ));
        }
        let key = self
            .keys
            .existing()
            .map_err(|error| failed("the database key could not be had", &error))?;
        custody::confirm_pending(&self.data_dir, code, &key)
            .map_err(|error| failed("the recovery envelope did not confirm", &error))?;
        *custody = Custody::Waiting(SetupStage::StoreNotSetUp);
        Ok(())
    }
}

fn already_set_up() -> IpcError {
    IpcError::new("already_set_up", "error.provisioning.already_set_up")
}

/// A provisioning step that failed, logged through the scrubbing layer and
/// answered with a fixed detail. The errors carry no key and no code.
fn failed(detail: &'static str, error: &dyn std::fmt::Display) -> IpcError {
    tracing::error!(error = %error, detail, "provisioning failed");
    IpcError::new("provisioning_failed", "error.provisioning.failed").with_detail(detail)
}

/// A random id for one showing of a code.
fn random_id() -> IpcResult<Uuid> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| failed("no id could be drawn", &error))?;
    Ok(uuid::Builder::from_random_bytes(bytes).into_uuid())
}

#[tauri::command]
pub fn provision_recovery_code(till: State<'_, Till>) -> IpcResult<RecoveryCodeDisplay> {
    till.provisioning()?.issue()
}

#[tauri::command]
pub fn acknowledge_recovery_code(till: State<'_, Till>, provisioning_id: Uuid) -> IpcResult<()> {
    till.provisioning()?.confirm(provisioning_id)
}

/// What [`Till::provisioning`] answers for a till that has a register.
pub(crate) fn not_provisioning() -> IpcError {
    already_set_up()
}

