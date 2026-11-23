-- SparrowShell 全新数据库初始化脚本
-- 适用版本：MySQL 5.7
-- 用途：创建 sparrowshell 数据库和全部数据表。
-- 注意：仅用于全新部署，不要在已有业务数据库上重复执行。
-- 后台服务启动时不会自动执行本脚本。

CREATE DATABASE IF NOT EXISTS sparrowshell
  CHARACTER SET utf8mb4
  COLLATE utf8mb4_unicode_ci;

USE sparrowshell;
SET NAMES utf8mb4;

-- ============================================================
-- V1：AI 账户额度与使用记录
-- ============================================================
CREATE TABLE IF NOT EXISTS ai_account_quota (
  account_id VARCHAR(128) NOT NULL,
  quota_limit DECIMAL(20,8) NOT NULL,
  reserved_cost DECIMAL(20,8) NOT NULL DEFAULT 0.00000000,
  created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
  PRIMARY KEY (account_id),
  CONSTRAINT chk_ai_quota_nonnegative CHECK (quota_limit >= 0 AND reserved_cost >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_usage_ledger (
  id BIGINT NOT NULL AUTO_INCREMENT,
  account_id VARCHAR(128) NOT NULL,
  client_request_id VARCHAR(64) NOT NULL,
  request_fingerprint CHAR(64) NOT NULL,
  route_id VARCHAR(64) NOT NULL,
  operation VARCHAR(16) NOT NULL,
  status VARCHAR(16) NOT NULL,
  estimated_cost DECIMAL(20,8) NOT NULL,
  input_tokens BIGINT NULL,
  output_tokens BIGINT NULL,
  actual_cost DECIMAL(20,8) NULL,
  usage_status VARCHAR(16) NOT NULL,
  price_version VARCHAR(64) NOT NULL,
  provider_request_id VARCHAR(128) NULL,
  error_code VARCHAR(64) NULL,
  created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
  PRIMARY KEY (id),
  UNIQUE KEY uk_ai_usage_account_request (account_id, client_request_id),
  KEY idx_ai_usage_created_route (created_at, route_id),
  KEY idx_ai_usage_status (status),
  CONSTRAINT fk_ai_usage_account FOREIGN KEY (account_id) REFERENCES ai_account_quota (account_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V2：账号、设备会话、云同步、版本发布、后台任务与账号注销
-- ============================================================
CREATE TABLE IF NOT EXISTS app_account (
  account_id CHAR(36) NOT NULL,
  status VARCHAR(24) NOT NULL DEFAULT 'active',
  created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  deleted_at TIMESTAMP(6) NULL DEFAULT NULL,
  PRIMARY KEY (account_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_device_session (
  session_id CHAR(36) NOT NULL,
  account_id CHAR(36) NOT NULL,
  device_id CHAR(36) NOT NULL,
  created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  revoked_at TIMESTAMP(6) NULL DEFAULT NULL,
  PRIMARY KEY (session_id),
  KEY idx_device_session_account (account_id, revoked_at),
  CONSTRAINT fk_device_session_account FOREIGN KEY (account_id) REFERENCES app_account (account_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_sync_entity (
  user_id CHAR(36) NOT NULL,
  entity_type VARCHAR(32) NOT NULL,
  entity_id CHAR(36) NOT NULL,
  revision BIGINT NOT NULL,
  change_seq BIGINT NOT NULL,
  deleted_at TIMESTAMP(6) NULL DEFAULT NULL,
  updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
  PRIMARY KEY (user_id, entity_type, entity_id),
  UNIQUE KEY uk_sync_entity_change (user_id, change_seq),
  KEY idx_sync_entity_deleted (user_id, deleted_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_update_release (
  id CHAR(36) NOT NULL,
  version VARCHAR(32) NOT NULL,
  minimum_macos VARCHAR(32) NOT NULL,
  artifact_url VARCHAR(2048) NOT NULL,
  artifact_bytes BIGINT NOT NULL,
  sha256 CHAR(64) NOT NULL,
  signature TEXT NOT NULL,
  channel VARCHAR(16) NOT NULL,
  update_policy VARCHAR(16) NOT NULL DEFAULT 'optional',
  release_notes TEXT NOT NULL,
  target VARCHAR(32) NOT NULL,
  state VARCHAR(16) NOT NULL,
  rollout_percent INT NOT NULL DEFAULT 0,
  published_at TIMESTAMP(6) NULL DEFAULT NULL,
  created_at DATETIME(6) NOT NULL,
  updated_at DATETIME(6) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uk_update_version_channel (version, channel),
  KEY idx_update_visible (channel, target, state),
  CONSTRAINT chk_update_target CHECK (target = 'darwin-aarch64'),
  CONSTRAINT chk_update_rollout CHECK (rollout_percent BETWEEN 0 AND 100)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_background_job (
  job_id CHAR(36) NOT NULL,
  job_type VARCHAR(64) NOT NULL,
  payload TEXT NOT NULL,
  dedupe_key VARCHAR(128) NOT NULL,
  status VARCHAR(24) NOT NULL,
  available_at DATETIME(6) NOT NULL,
  lease_owner VARCHAR(160) NULL DEFAULT NULL,
  lease_until TIMESTAMP(6) NULL DEFAULT NULL,
  attempts INT NOT NULL DEFAULT 0,
  max_attempts INT NOT NULL,
  last_error_code VARCHAR(128) NULL DEFAULT NULL,
  created_at DATETIME(6) NOT NULL,
  completed_at TIMESTAMP(6) NULL DEFAULT NULL,
  PRIMARY KEY (job_id),
  UNIQUE KEY uk_background_job_dedupe (job_type, dedupe_key),
  KEY idx_background_job_claim (status, available_at, lease_until, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_account_deletion_step (
  account_id CHAR(36) NOT NULL,
  step VARCHAR(40) NOT NULL,
  completed_at DATETIME(6) NOT NULL,
  PRIMARY KEY (account_id, step),
  CONSTRAINT fk_deletion_step_account FOREIGN KEY (account_id) REFERENCES app_account (account_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V3：账号资料与后台管理端
-- ============================================================
ALTER TABLE app_account
  ADD COLUMN email VARCHAR(254) NULL DEFAULT NULL AFTER account_id,
  ADD COLUMN nickname VARCHAR(120) NULL DEFAULT NULL AFTER email,
  ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 AFTER status,
  ADD COLUMN disabled_reason VARCHAR(500) NULL DEFAULT NULL AFTER revision,
  ADD COLUMN last_active_at TIMESTAMP(6) NULL DEFAULT NULL AFTER created_at;
CREATE UNIQUE INDEX uk_app_account_email ON app_account (email);

ALTER TABLE app_device_session
  ADD COLUMN device_name VARCHAR(160) NULL DEFAULT NULL AFTER device_id,
  ADD COLUMN client_version VARCHAR(40) NULL DEFAULT NULL AFTER device_name,
  ADD COLUMN last_seen_at TIMESTAMP(6) NULL DEFAULT NULL AFTER created_at;
CREATE INDEX idx_device_session_seen ON app_device_session (last_seen_at);

CREATE TABLE IF NOT EXISTS admin_user (
  id CHAR(36) NOT NULL,
  email VARCHAR(254) NOT NULL,
  password_hash VARCHAR(128) NOT NULL,
  password_salt VARCHAR(64) NOT NULL,
  mfa_secret VARCHAR(160) NOT NULL,
  role VARCHAR(40) NOT NULL,
  status VARCHAR(24) NOT NULL DEFAULT 'active',
  created_at DATETIME(6) NOT NULL,
  last_login_at TIMESTAMP(6) NULL DEFAULT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uk_admin_user_email (email)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS admin_session (
  session_id CHAR(36) NOT NULL,
  admin_id CHAR(36) NOT NULL,
  token_hash CHAR(44) NOT NULL,
  created_at DATETIME(6) NOT NULL,
  expires_at DATETIME(6) NOT NULL,
  revoked_at TIMESTAMP(6) NULL DEFAULT NULL,
  PRIMARY KEY (session_id),
  UNIQUE KEY uk_admin_session_token (token_hash),
  KEY idx_admin_session_admin (admin_id, revoked_at),
  CONSTRAINT fk_admin_session_user FOREIGN KEY (admin_id) REFERENCES admin_user (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS admin_audit (
  id CHAR(36) NOT NULL,
  operation_id VARCHAR(100) NOT NULL,
  admin_id CHAR(36) NOT NULL,
  admin_email VARCHAR(254) NOT NULL,
  action VARCHAR(64) NOT NULL,
  target_type VARCHAR(32) NOT NULL,
  target_id VARCHAR(100) NOT NULL,
  reason VARCHAR(500) NOT NULL,
  result VARCHAR(24) NOT NULL,
  error_code VARCHAR(100) NULL DEFAULT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uk_admin_audit_operation (admin_id,action,operation_id),
  KEY idx_admin_audit_created (created_at),
  CONSTRAINT fk_admin_audit_user FOREIGN KEY (admin_id) REFERENCES admin_user (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V4：密码登录、访问令牌与同步载荷
-- ============================================================
ALTER TABLE app_account
  ADD COLUMN password_hash VARCHAR(128) NULL DEFAULT NULL AFTER nickname,
  ADD COLUMN password_salt VARCHAR(64) NULL DEFAULT NULL AFTER password_hash;

ALTER TABLE app_device_session
  ADD COLUMN token_hash CHAR(44) NULL DEFAULT NULL AFTER client_version,
  ADD COLUMN expires_at DATETIME(6) NULL DEFAULT NULL AFTER token_hash;
CREATE UNIQUE INDEX uk_app_device_token ON app_device_session (token_hash);
CREATE INDEX idx_app_device_lookup ON app_device_session (account_id, device_id, revoked_at);

ALTER TABLE app_sync_entity
  ADD COLUMN schema_version INT NOT NULL DEFAULT 1 AFTER revision,
  ADD COLUMN payload MEDIUMTEXT NULL DEFAULT NULL AFTER deleted_at;

CREATE TABLE IF NOT EXISTS app_sync_receipt (
  user_id CHAR(36) NOT NULL,
  mutation_id CHAR(36) NOT NULL,
  response_json MEDIUMTEXT NOT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (user_id, mutation_id),
  CONSTRAINT fk_sync_receipt_account FOREIGN KEY (user_id) REFERENCES app_account (account_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V5：刷新令牌轮换与吊销
-- ============================================================
CREATE TABLE IF NOT EXISTS app_refresh_token (
  token_hash CHAR(44) NOT NULL,
  session_id CHAR(36) NOT NULL,
  generation BIGINT NOT NULL,
  expires_at DATETIME(6) NOT NULL,
  rotated_at DATETIME(6) NULL DEFAULT NULL,
  revoked_at DATETIME(6) NULL DEFAULT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (token_hash),
  UNIQUE KEY uk_refresh_session_generation (session_id, generation),
  KEY idx_refresh_session_state (session_id, revoked_at, rotated_at),
  CONSTRAINT fk_refresh_session FOREIGN KEY (session_id) REFERENCES app_device_session (session_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V6：云同步请求观测记录
-- 仅保存协议元数据，不保存同步载荷、服务器配置、终端或 AI 内容。
-- ============================================================
CREATE TABLE IF NOT EXISTS app_sync_request (
  request_id CHAR(36) NOT NULL,
  user_id CHAR(36) NULL DEFAULT NULL,
  device_id CHAR(36) NULL DEFAULT NULL,
  operation VARCHAR(16) NOT NULL,
  status VARCHAR(16) NOT NULL,
  error_code VARCHAR(64) NULL DEFAULT NULL,
  http_status INT NOT NULL,
  revision BIGINT NULL DEFAULT NULL,
  change_seq BIGINT NULL DEFAULT NULL,
  conflict_count INT NOT NULL DEFAULT 0,
  cursor_expired BOOLEAN NOT NULL DEFAULT FALSE,
  batch_size INT NOT NULL DEFAULT 0,
  duration_ms BIGINT NOT NULL,
  client_version VARCHAR(40) NULL DEFAULT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (request_id),
  KEY idx_sync_request_created (created_at, operation, status),
  KEY idx_sync_request_user (user_id, created_at),
  KEY idx_sync_request_error (error_code, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V7：问题反馈与附件
-- ============================================================
CREATE TABLE IF NOT EXISTS app_feedback (
  id CHAR(36) NOT NULL,
  user_id CHAR(36) NULL DEFAULT NULL,
  description TEXT NOT NULL,
  contact VARCHAR(320) NULL DEFAULT NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'pending',
  created_at DATETIME(6) NOT NULL,
  updated_at DATETIME(6) NOT NULL,
  updated_by CHAR(36) NULL DEFAULT NULL,
  PRIMARY KEY (id),
  KEY idx_feedback_user (user_id, created_at),
  KEY idx_feedback_status (status, created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS app_feedback_attachment (
  feedback_id CHAR(36) NOT NULL,
  original_name VARCHAR(255) NOT NULL,
  storage_key VARCHAR(255) NOT NULL,
  content_type VARCHAR(100) NOT NULL,
  size_bytes BIGINT NOT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (feedback_id),
  UNIQUE KEY uk_feedback_attachment_storage (storage_key),
  CONSTRAINT fk_feedback_attachment_feedback FOREIGN KEY (feedback_id) REFERENCES app_feedback (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V8、V9：更新策略与发布说明
-- V2 的 app_update_release 建表语句已包含 update_policy 和 release_notes，
-- 全新初始化时不能再次执行 ADD COLUMN，否则会产生重复字段错误。
-- ============================================================

-- ============================================================
-- V10：邮箱验证码
-- 仅保存验证码的 SHA-256 哈希，验证码有效期为五分钟。
-- ============================================================
CREATE TABLE IF NOT EXISTS app_email_verification (
  id CHAR(36) NOT NULL,
  email VARCHAR(254) NOT NULL,
  purpose VARCHAR(32) NOT NULL,
  code_hash CHAR(44) NOT NULL,
  attempts INT NOT NULL DEFAULT 0,
  created_at DATETIME(6) NOT NULL,
  expires_at DATETIME(6) NOT NULL,
  used_at DATETIME(6) NULL DEFAULT NULL,
  PRIMARY KEY (id),
  KEY idx_email_verification_latest (email, purpose, created_at),
  KEY idx_email_verification_expiry (expires_at, used_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V11：AI 套餐、支付宝订单、许可证与额度
-- 激活码和设备令牌不以明文保存；key_ciphertext 仅用于已支付页面再次展示激活码。
-- ============================================================
CREATE TABLE IF NOT EXISTS ai_billing_order (
  id CHAR(36) NOT NULL,
  order_token CHAR(32) NOT NULL,
  plan VARCHAR(24) NOT NULL,
  amount DECIMAL(10,2) NOT NULL,
  status VARCHAR(24) NOT NULL,
  license_id CHAR(36) NULL,
  qr_code TEXT NULL,
  qr_expires_at DATETIME(6) NULL,
  order_expires_at DATETIME(6) NOT NULL,
  alipay_trade_no VARCHAR(64) NULL,
  created_at DATETIME(6) NOT NULL,
  paid_at DATETIME(6) NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uk_ai_billing_order_token (order_token),
  UNIQUE KEY uk_ai_billing_order_trade (alipay_trade_no),
  KEY idx_ai_billing_order_status (status, order_expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_license (
  id CHAR(36) NOT NULL,
  key_hash CHAR(64) NOT NULL,
  key_ciphertext TEXT NOT NULL,
  status VARCHAR(24) NOT NULL,
  bound_device_id CHAR(36) NULL,
  device_token_hash CHAR(64) NULL,
  created_at DATETIME(6) NOT NULL,
  updated_at DATETIME(6) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uk_ai_license_key_hash (key_hash),
  UNIQUE KEY uk_ai_license_bound_device (bound_device_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_license_period (
  id CHAR(36) NOT NULL,
  license_id CHAR(36) NOT NULL,
  plan VARCHAR(24) NOT NULL,
  quota INT NOT NULL,
  used INT NOT NULL DEFAULT 0,
  starts_at DATETIME(6) NOT NULL,
  ends_at DATETIME(6) NOT NULL,
  created_at DATETIME(6) NOT NULL,
  PRIMARY KEY (id),
  KEY idx_ai_license_period_active (license_id, starts_at, ends_at),
  CONSTRAINT fk_ai_license_period_license FOREIGN KEY (license_id) REFERENCES ai_license(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_free_quota (
  device_id CHAR(36) NOT NULL,
  quota_month CHAR(7) NOT NULL,
  quota INT NOT NULL,
  used INT NOT NULL DEFAULT 0,
  updated_at DATETIME(6) NOT NULL,
  PRIMARY KEY (device_id, quota_month)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_task_usage (
  task_id CHAR(36) NOT NULL,
  device_id CHAR(36) NOT NULL,
  period_id CHAR(36) NULL,
  quota_month CHAR(7) NULL,
  status VARCHAR(24) NOT NULL,
  created_at DATETIME(6) NOT NULL,
  updated_at DATETIME(6) NOT NULL,
  expires_at DATETIME(6) NOT NULL,
  PRIMARY KEY (task_id, device_id),
  KEY idx_ai_task_usage_expiry (status, expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS ai_payment_notification (
  notification_id VARCHAR(128) NOT NULL,
  order_token CHAR(32) NOT NULL,
  trade_no VARCHAR(64) NOT NULL,
  received_at DATETIME(6) NOT NULL,
  PRIMARY KEY (notification_id),
  UNIQUE KEY uk_ai_payment_notification_trade (trade_no)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================
-- V12：订阅列表、生命周期筛选与到期监控索引
-- ============================================================
ALTER TABLE ai_license_period
  ADD INDEX idx_ai_license_period_timeline (starts_at, ends_at, plan);

-- ============================================================
-- V13：许可证首次激活时间
-- ============================================================
ALTER TABLE ai_license
  ADD COLUMN activated_at DATETIME(6) NULL AFTER updated_at;

UPDATE ai_license
SET activated_at = updated_at
WHERE bound_device_id IS NOT NULL OR device_token_hash IS NOT NULL;

-- ============================================================
-- V14：支持 Mac 与 Windows 独立发布同一版本号
-- 兼容 MySQL 5.7：仅当旧 CHECK 约束实际存在时才删除。
-- ============================================================
ALTER TABLE app_update_release
  DROP INDEX uk_update_version_channel,
  ADD UNIQUE KEY uk_update_version_channel_target (version, channel, target);

SET @drop_update_target_check = (
  SELECT IF(
    COUNT(*) > 0,
    'ALTER TABLE app_update_release DROP CHECK chk_update_target',
    'SELECT 1'
  )
  FROM information_schema.TABLE_CONSTRAINTS
  WHERE TABLE_SCHEMA = DATABASE()
    AND TABLE_NAME = 'app_update_release'
    AND CONSTRAINT_NAME = 'chk_update_target'
);
PREPARE drop_update_target_check FROM @drop_update_target_check;
EXECUTE drop_update_target_check;
DEALLOCATE PREPARE drop_update_target_check;

-- 初始化完成。本脚本不会创建默认管理员；首个超管请按部署文档使用一次性管理员引导创建。
