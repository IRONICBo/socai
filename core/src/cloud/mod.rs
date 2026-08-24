//! Managed socai services shared by the CLI and desktop app.

mod asr;
mod auth;
mod billing;
mod browser;
mod support;

pub use asr::{transcribe_audio_file, CloudAsrResult};
pub use auth::{
    activate, activate_with_base_url, auth_session, ensure_trial_device, hosted_llm_selected,
    llm_gateway_config, logout, pro_activated, redeem_invite, reset_llm_gateway_for_task,
    send_sms_code, set_hosted_llm_selected, status, take_hosted_llm_default, trial_available,
    verify_sms_code, AuthSession, CloudCredentials, CloudStatus, InviteRedemption,
    LlmGatewayConfig, SmsChallengeResponse,
};
pub use billing::{
    create_alipay_order, create_wechat_order, mock_recharge, payment_order, payment_plan,
    settle_llm_task, wallet_balance, LlmSettlement, PaymentOrder, PaymentPlan, RechargeReceipt,
    WalletBalance,
};
pub use browser::{create_browser_session, release_browser_session, BrowserSessionInfo};
pub use support::{diagnose_error, ErrorDiagnosis};

pub(crate) use auth::llm_gateway_config_for_task;
pub(crate) use auth::telemetry_account_snapshot;
