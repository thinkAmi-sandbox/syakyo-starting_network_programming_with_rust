use std::env;

// [macro_use]でマクロのインポート
// (use <マクロ> でインポートできるが、うまくいかない場合もある
//   https://qiita.com/dalance/items/e736f642460ae74d506e
// クレートをリンクする
//   ライブラリをリンクするだけでなく、その要素を全てライブラリと同じ名前のモジュールにインポート
//   https://doc.rust-jp.rs/rust-by-example-ja/crates/link.html
#[macro_use]
extern crate log;

// tcp_clientモジュールを参照する
//   https://keens.github.io/blog/2018/12/08/rustnomoju_runotsukaikata_2018_editionhan/
mod tcp_client;
mod tcp_server;
mod udp_client;
mod udp_server;

// コマンド引数に応じて、各モジュールの関数を呼び出す
fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // letで型推論
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        error!("Process specify [tcp|udp] [server|client] [addr:port].");
        std::process::exit(1);
    }

    // ":" を使って、変数の後ろに型宣言 (&str型:文字列を扱う)
    let protocol: &str = &args[1];
    let role: &str = &args[2];
    let address = &args[3];

    // match文で処理分岐
    match protocol {
        "tcp" => match role {
            "server" => {
                // 末尾セミコロンが無いと式、セミコロンがあると文
                tcp_server::serve(address).unwrap_or_else(|e| error!("{}", e));
            }
            "client" => {
                tcp_client::connect(address).unwrap_or_else(|e| error!("{}", e));
            }
            _ => {
                missing_role();
            }
        },
        "udp" => match role {
            "server" => {
                udp_server::serve(address).unwrap_or_else(|e| error!("{}", e));
            }
            "client" => {
                udp_client::communicate(address).unwrap_or_else(|e| error!("{}", e));
            }
            _ => {
                missing_role();
            }
        },
        // その他の処理
        _ => {
            error!("Please specify tcp or udp on the 1st argument.");
            std::process::exit(1);
        }
    }
}

/**
 * 第2引数が不正な時にエラーを出す関数
 */
// "fn" で関数宣言
fn missing_role() {
    error!("Please specify server or client on the 2nd argument.");
    std::process::exit(1);
}
