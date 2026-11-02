//! OS-owned credential storage. Never serialize secrets or expose backend errors.
use zeroize::Zeroizing;

const SERVICE: &str = "Big Picture";
const ACCOUNT: &str = "openai-api-key";

pub(super) enum Action {
    Load,
    Save(Zeroizing<String>),
    Forget,
}

pub(super) enum Outcome {
    Loaded(Option<Zeroizing<String>>),
    Saved,
    Forgotten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Error {
    Unavailable,
    InvalidKey,
}

pub(super) fn store_name() -> &'static str {
    if cfg!(windows) {
        "Windows Credential Manager"
    } else if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "linux") {
        "Secret Service"
    } else {
        "OS credential store"
    }
}

pub(super) fn execute(action: Action) -> Result<Outcome, Error> {
    // keyring otherwise silently defaults to its mock store on unsupported OSes.
    // Explicitly refuse those platforms rather than claiming a secret is saved.
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| Error::Unavailable)?;
        perform(&entry, action)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = action;
        Err(Error::Unavailable)
    }
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn perform(entry: &keyring::Entry, action: Action) -> Result<Outcome, Error> {
    match action {
        Action::Load => match entry.get_password() {
            Ok(key) => {
                let key = Zeroizing::new(key);
                validate_key(&key)?;
                Ok(Outcome::Loaded(Some(key)))
            }
            Err(keyring::Error::NoEntry) => Ok(Outcome::Loaded(None)),
            // Some keyring error variants contain secret bytes. Never format them.
            Err(_) => Err(Error::Unavailable),
        },
        Action::Save(key) => {
            validate_key(&key)?;
            entry
                .set_password(key.trim())
                .map_err(|_| Error::Unavailable)?;
            Ok(Outcome::Saved)
        }
        Action::Forget => {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => return Err(Error::Unavailable),
            }
            Ok(Outcome::Forgotten)
        }
    }
}

fn validate_key(key: &str) -> Result<(), Error> {
    if key.trim().is_empty() || key.trim().chars().any(char::is_whitespace) {
        Err(Error::InvalidKey)
    } else {
        Ok(())
    }
}

#[cfg(all(test, any(windows, target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn isolated_store_handles_save_load_replace_forget_and_missing_entries() {
        // Construct a private mock entry; never change the global builder or
        // touch a user's real credential in ordinary automated tests.
        let entry =
            keyring::Entry::new_with_credential(Box::new(keyring::mock::MockCredential::default()));
        assert!(matches!(
            perform(&entry, Action::Load),
            Ok(Outcome::Loaded(None))
        ));
        assert!(matches!(
            perform(&entry, Action::Save(Zeroizing::new(" test-key ".into()))),
            Ok(Outcome::Saved)
        ));
        let Ok(Outcome::Loaded(Some(key))) = perform(&entry, Action::Load) else {
            panic!("saved key missing")
        };
        assert!(key.as_str() == "test-key");
        assert!(perform(&entry, Action::Save(Zeroizing::new("replacement".into()))).is_ok());
        let Ok(Outcome::Loaded(Some(key))) = perform(&entry, Action::Load) else {
            panic!("replacement key missing")
        };
        assert!(key.as_str() == "replacement");
        assert!(matches!(
            perform(&entry, Action::Forget),
            Ok(Outcome::Forgotten)
        ));
        assert!(matches!(
            perform(&entry, Action::Load),
            Ok(Outcome::Loaded(None))
        ));
        assert!(perform(&entry, Action::Forget).is_ok());
        assert!(matches!(
            perform(&entry, Action::Save(Zeroizing::new("bad key".into()))),
            Err(Error::InvalidKey)
        ));
        let mock = entry
            .get_credential()
            .downcast_ref::<keyring::mock::MockCredential>()
            .unwrap();
        mock.set_error(keyring::Error::BadEncoding(
            b"private-data-in-backend-error".to_vec(),
        ));
        assert!(matches!(
            perform(&entry, Action::Load),
            Err(Error::Unavailable)
        ));
    }

    #[test]
    #[ignore = "explicit native-vault smoke test using a unique disposable dummy credential"]
    fn native_credential_round_trip_with_fresh_handles() {
        let user = format!(
            "test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let service = "Big Picture disposable credential test";
        let entry = || keyring::Entry::new(service, &user).expect("native entry unavailable");
        struct Cleanup(keyring::Entry);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete_credential();
            }
        }
        let _cleanup = Cleanup(entry());
        assert!(matches!(
            perform(
                &entry(),
                Action::Save(Zeroizing::new("dummy-not-an-api-key".into()))
            ),
            Ok(Outcome::Saved)
        ));
        let Ok(Outcome::Loaded(Some(key))) = perform(&entry(), Action::Load) else {
            panic!("native reload failed")
        };
        assert!(key.as_str() == "dummy-not-an-api-key");
        assert!(perform(&entry(), Action::Forget).is_ok());
        assert!(matches!(
            perform(&entry(), Action::Load),
            Ok(Outcome::Loaded(None))
        ));
    }
}
