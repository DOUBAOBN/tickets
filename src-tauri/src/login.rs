use crate::clock;
use crate::http::{self, Account};
use serde::Serialize;
use serde_json::{json, Value};

const ORIGIN: &str = "https://m.damai.cn";
const APP_KEY: &str = "12574478";
const PROBE_ITEM: &str = "1089383688003";
const LOGIN_WINDOW: &str = "damai-login";
const LOGIN_PAGE: &str = "https://passport.damai.cn/login";
const DETAIL_API: &str = "mtop.alibaba.damai.detail.getdetail";
// 只有登录后才会下发的 Cookie，用来判断扫码是否完成
const LOGIN_MARKERS: [&str; 5] = ["unb", "login", "_nk_", "damai.cn_user", "loginkey"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginState {
    pub ready: bool,
    pub cookie: String,
}

fn pairs(cookie: &str) -> Vec<(String, String)> {
    cookie
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn join(cookies: &[(String, String)]) -> String {
    cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn logged_in(cookies: &[(String, String)]) -> bool {
    cookies
        .iter()
        .any(|(name, value)| !value.is_empty() && LOGIN_MARKERS.contains(&name.as_str()))
}

fn gateway_message(body: &str) -> Option<String> {
    let message = serde_json::from_str::<Value>(body)
        .ok()?
        .get("ret")?
        .as_array()?
        .iter()
        .map(http::string)
        .collect::<Vec<_>>()
        .join("；");
    (!message.is_empty()).then_some(message)
}

/// 用已有登录态向 mtop 网关换一份新的 _m_h5_tk / _m_h5_tk_enc。
/// 旧令牌先丢弃，首个请求按空令牌签名，网关会随响应把新令牌下发回来。
async fn bootstrap(account: &Account) -> Result<String, String> {
    let kept: Vec<(String, String)> = pairs(&account.cookie)
        .into_iter()
        .filter(|(name, _)| !name.starts_with("_m_h5_tk"))
        .collect();
    if kept.is_empty() {
        return Err("没有可用的登录凭证，请先登录大麦或粘贴完整 Cookie".into());
    }
    let base = Account {
        cookie: join(&kept),
        ..account.clone()
    };
    let data = json!({
        "itemId": PROBE_ITEM,
        "bizCode": "ali.china.damai",
        "scenario": "itemsku",
        "exParams": json!({ "dataType": 4, "dataId": "", "privilegeActId": "" }).to_string(),
        "dmChannel": "damai@damaih5_h5",
    })
    .to_string();
    let t = clock::now_ms().to_string();
    let signature = format!("{:x}", md5::compute(format!("&{t}&{APP_KEY}&{data}")));
    let query = [
        ("jsv", "2.7.2"),
        ("appKey", APP_KEY),
        ("t", t.as_str()),
        ("sign", signature.as_str()),
        ("api", DETAIL_API),
        ("v", "1.2"),
        ("type", "originaljson"),
        ("dataType", "json"),
        ("H5Request", "true"),
        ("AntiCreep", "true"),
        ("AntiFlood", "true"),
    ];
    let response = http::send(
        http::client(&base, ORIGIN)?
            .post(format!("https://mtop.damai.cn/h5/{DETAIL_API}/1.2/"))
            .query(&query)
            .form(&[("data", data.as_str())]),
    )
    .await?;
    let issued: Vec<String> = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect();
    let body = response.text().await.unwrap_or_default();
    let mut merged = kept;
    for value in &issued {
        let pair = value.split(';').next().unwrap_or_default();
        if let Some((name, value)) = pair.split_once('=') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            merged.retain(|(existing, _)| existing.as_str() != name);
            merged.push((name.to_string(), value.trim().to_string()));
        }
    }
    if !merged.iter().any(|(name, _)| name.as_str() == "_m_h5_tk") {
        return Err(match gateway_message(&body) {
            Some(message) => format!("{message}；请在大麦页面重新登录后再试"),
            None => "网关没有下发 _m_h5_tk，请确认账号仍处于登录状态".into(),
        });
    }
    Ok(join(&merged))
}

#[cfg(windows)]
mod desktop {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tauri::Window;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::{HSTRING, PCWSTR};

    /// 通过 DevTools 协议直接读取 WebView2 的 Cookie，HttpOnly 的也能拿到。
    /// 回调由主线程消息循环触发，所以这里只发起请求，结果经 oneshot 送回。
    async fn devtools(window: &Window, method: &str, parameters: &str) -> Result<String, String> {
        let method = method.to_string();
        let parameters = parameters.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel::<Result<String, String>>();
        let slot = Arc::new(Mutex::new(Some(sender)));
        let callback = slot.clone();
        window
            .with_webview(move |webview| {
                let core = match unsafe { webview.controller().CoreWebView2() } {
                    Ok(core) => core,
                    Err(_) => return,
                };
                let handler =
                    CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, body| {
                        let value = match result {
                            Ok(()) => Ok(body),
                            Err(_) => Err("登录窗口没有返回凭证，请重试".to_string()),
                        };
                        let mut slot = callback.lock().unwrap_or_else(|error| error.into_inner());
                        if let Some(sender) = slot.take() {
                            let _ = sender.send(value);
                        }
                        Ok(())
                    }));
                let _ = unsafe {
                    core.CallDevToolsProtocolMethod(
                        PCWSTR::from(&HSTRING::from(method)),
                        PCWSTR::from(&HSTRING::from(parameters)),
                        &handler,
                    )
                };
            })
            .map_err(|_| "无法访问登录窗口，请重试".to_string())?;
        tokio::time::timeout(Duration::from_secs(10), receiver)
            .await
            .map_err(|_| "读取登录凭证超时，请重试".to_string())?
            .map_err(|_| "登录窗口已关闭，请重新打开".to_string())?
    }

    fn parse(body: &str) -> Vec<(String, String, String)> {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| value.get("cookies")?.as_array().cloned())
            .map(|cookies| {
                cookies
                    .iter()
                    .filter_map(|cookie| {
                        let name = cookie["name"].as_str()?.trim();
                        let value = cookie["value"].as_str()?;
                        if name.is_empty() {
                            return None;
                        }
                        let domain = cookie["domain"].as_str().unwrap_or_default();
                        Some((name.to_string(), value.to_string(), domain.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn cookies(window: &Window) -> Result<Vec<(String, String)>, String> {
        let urls = json!({ "urls": [
            "https://www.damai.cn/",
            "https://m.damai.cn/",
            "https://passport.damai.cn/",
            "https://mtop.damai.cn/",
            "https://orders.damai.cn/",
        ] })
        .to_string();
        let scoped: Vec<(String, String)> = parse(&devtools(window, "Network.getCookies", &urls).await?)
            .into_iter()
            .map(|(name, value, _)| (name, value))
            .collect();
        if !scoped.is_empty() {
            return Ok(scoped);
        }
        // 部分 WebView2 版本上按 URL 查询不可用，退回全量再按域名筛选
        let all: Vec<(String, String)> = parse(&devtools(window, "Network.getAllCookies", "{}").await?)
            .into_iter()
            .filter(|(_, _, domain)| {
                let domain = domain.trim_start_matches('.');
                domain == "damai.cn" || domain.ends_with(".damai.cn")
            })
            .map(|(name, value, _)| (name, value))
            .collect();
        Ok(all)
    }
}

#[cfg(windows)]
async fn open_login_window(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_window(LOGIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    tauri::WindowBuilder::new(
        app,
        LOGIN_WINDOW,
        tauri::WindowUrl::External(LOGIN_PAGE.parse().map_err(|_| "登录地址无效")?),
    )
    .title("登录大麦")
    .inner_size(560.0, 800.0)
    .min_inner_size(420.0, 600.0)
    .center()
    .build()
    .map_err(|_| "无法打开登录窗口，请重试")?;
    Ok(())
}

#[cfg(windows)]
async fn check_login(app: &tauri::AppHandle) -> Result<LoginState, String> {
    use tauri::Manager;
    let window = app.get_window(LOGIN_WINDOW).ok_or("登录窗口已关闭，请重新打开")?;
    let cookies = desktop::cookies(&window).await?;
    if !logged_in(&cookies) {
        return Ok(LoginState {
            ready: false,
            cookie: String::new(),
        });
    }
    let account = Account {
        cookie: join(&cookies),
        proxy: String::new(),
        subscription_id: String::new(),
        subscription_node_id: String::new(),
    };
    let cookie = bootstrap(&account).await?;
    let _ = window.close();
    Ok(LoginState {
        ready: true,
        cookie,
    })
}

#[cfg(not(windows))]
async fn open_login_window(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("扫码登录仅支持 Windows 桌面版".into())
}

#[cfg(not(windows))]
async fn check_login(_app: &tauri::AppHandle) -> Result<LoginState, String> {
    Err("扫码登录仅支持 Windows 桌面版".into())
}

/// 打开大麦登录窗口，扫码或密码登录都可以。
#[tauri::command]
pub async fn dm_login(app: tauri::AppHandle) -> Result<(), String> {
    open_login_window(&app).await
}

/// 读取登录窗口里的登录态。登录完成时返回可直接保存的完整 Cookie。
#[tauri::command]
pub async fn dm_login_check(app: tauri::AppHandle) -> Result<LoginState, String> {
    check_login(&app).await
}

/// 复用已保存的登录态，只换一份新的 _m_h5_tk。
#[tauri::command]
pub async fn dm_refresh_token(account: Account) -> Result<String, String> {
    bootstrap(&account).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_round_trip_and_detect_login() {
        let cookie = "cna=abc; _m_h5_tk=deadbeef_1700000000000; unb=123456; _nk_=%E5%A4%A7%E9%BA%A6";
        let parsed = pairs(cookie);
        assert_eq!(parsed.len(), 4);
        assert!(logged_in(&parsed));
        assert!(!logged_in(&pairs("cna=abc; cookie2=xyz")));
        assert_eq!(join(&pairs("a=1; b=2")), "a=1; b=2");
    }

    #[test]
    fn bootstrap_drops_stale_tokens_and_keeps_the_rest() {
        let kept: Vec<(String, String)> = pairs("a=1; _m_h5_tk=t_1; _m_h5_tk_enc=e; b=2")
            .into_iter()
            .filter(|(name, _)| !name.starts_with("_m_h5_tk"))
            .collect();
        assert_eq!(join(&kept), "a=1; b=2");
    }

    #[test]
    fn gateway_message_reads_ret_array() {
        assert_eq!(
            gateway_message(r#"{"ret":["FAIL_SYS_SESSION_EXPIRED::Session过期"],"data":{}}"#),
            Some("FAIL_SYS_SESSION_EXPIRED::Session过期".into())
        );
        assert_eq!(gateway_message(r#"{"data":{}}"#), None);
        assert_eq!(gateway_message("not json"), None);
    }
}
