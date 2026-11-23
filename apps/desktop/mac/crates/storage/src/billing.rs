use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::{Repository, StorageError};

const AI_LICENSE_KEY: &str = "ai_license_key";
const AI_DEVICE_TOKEN: &str = "ai_device_token";

fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn write_setting(connection: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO settings(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        params![key, value, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

impl Repository {
    pub fn ai_license_key(&self) -> Result<Option<String>, StorageError> {
        read_setting(&self.connection, AI_LICENSE_KEY)
    }

    pub fn ai_device_token(&self) -> Result<Option<String>, StorageError> {
        read_setting(&self.connection, AI_DEVICE_TOKEN)
    }

    pub fn save_ai_license_key(&self, license_key: &str) -> Result<(), StorageError> {
        write_setting(&self.connection, AI_LICENSE_KEY, license_key)
    }

    pub fn save_ai_device_token(&self, device_token: &str) -> Result<(), StorageError> {
        write_setting(&self.connection, AI_DEVICE_TOKEN, device_token)
    }

    pub fn save_ai_license(
        &self,
        license_key: &str,
        device_token: &str,
    ) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        write_setting(&transaction, AI_LICENSE_KEY, license_key)?;
        write_setting(&transaction, AI_DEVICE_TOKEN, device_token)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn clear_ai_license(&self) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM settings WHERE key IN (?1,?2)",
            params![AI_LICENSE_KEY, AI_DEVICE_TOKEN],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_license_is_local_replaceable_and_clearable_without_cloud_sync() {
        let repository = Repository::in_memory().unwrap();
        assert_eq!(repository.ai_license_key().unwrap(), None);
        assert_eq!(repository.ai_device_token().unwrap(), None);

        repository
            .save_ai_license("test-license", "test-device-token")
            .unwrap();
        assert_eq!(
            repository.ai_license_key().unwrap().as_deref(),
            Some("test-license")
        );
        assert_eq!(
            repository.ai_device_token().unwrap().as_deref(),
            Some("test-device-token")
        );

        repository
            .save_ai_license_key("replacement-license")
            .unwrap();
        repository
            .save_ai_device_token("replacement-device-token")
            .unwrap();
        assert_eq!(
            repository.ai_license_key().unwrap().as_deref(),
            Some("replacement-license")
        );
        assert_eq!(
            repository.ai_device_token().unwrap().as_deref(),
            Some("replacement-device-token")
        );

        repository.clear_ai_license().unwrap();
        assert_eq!(repository.ai_license_key().unwrap(), None);
        assert_eq!(repository.ai_device_token().unwrap(), None);
        assert_eq!(repository.outbox_len().unwrap(), 0);
    }
}
