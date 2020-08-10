// TCP Echoサーバ
// TCP通信：TCPListener, TCPStream

use std::io::{Read, Write};
// std::net::TcpListenerを、TcpListenerとして使えるようにする
use std::net::{TcpListener, TcpStream};
// std::strを、strとして使えるようにする
use std::{str, thread};

/**
 * 指定のソケットアドレスで接続を待ち受ける
 */
// 関数を使うときは型宣言が必須
// "->" で戻り値を指定
// &str で受け取った文字列は表示・format!による連結はできるが、文字列の中身は変えられない
// エラーが発生した時にエラーメッセージを返すため、Result型を使う
//   第一引数が本来返したい値、第２引数がエラーが発生したときの詳細情報
pub fn serve(address : &str) -> Result<(), failure::Error> {
    // TCPのコネクションを待ち受けるソケットを作成 (リスニングソケット or サーバソケット)
    let listener = TcpListener::bind(address)?;

    loop {
        // "?"演算子は、Resultを返す関数の中で使用可能
        //   値がOKなら中の値を返し、Errなら、即座に値をreturnする
        //   用途：Resultを受け取った時に、エラーなら処理をすぐに中断してreturnしたい時
        //   https://qiita.com/nirasan/items/321e7cc42e0e0f238254
        // (foo,bar) はタプル
        // コネクション確立済のソケットを返却 (存在していなければスレッドを停止)
        // 接続済ソケット(クライントソケット)を取得
        let (stream, _) = listener.accept()?;

        // コネクションごとにスレッドを立ち上げる
        thread::spawn(move || {
            // "||" はクロージャの引数
            // unwrap_or_else()で、エラーが起きた時にpanicを起こさずに、その中身をクロージャにて処理する
            //   クロージャを使わないなら、unwrap_or() を使う
            handler(stream).unwrap_or_else(|error| error!("{:?}", error));
        });
    }
}

/**
 * クライアントからの入力を待ち受け、受信したら同じものを返却する
 */
// 引数を "mut" で指定しておくことで、引数を変更可能にできる
fn handler(mut stream: TcpStream) -> Result<(), failure::Error> {
    // 日本語はスライスしなければ普通に扱える
    debug!("Handling data from {}", stream.peer_addr()?);

    // "mut" で、可変変数にしている
    // デフォルトでは、一度値を入れたら、後から変更できない
    // あるいはシャドーイング(let 同じ変数名) を使う
    // "0u8" は、 u8 (符号なし8bitの数値型の"0")。それを1024個用意している。
    //   https://stackoverflow.com/questions/53120755/what-does-0u8-mean-in-rust
    let mut buffer = [0u8; 1024];
    // loopで無限に繰り返す
    loop {
        // EOFに到達する(通信が切断される)と "0" を返す
        let nbytes = stream.read(&mut buffer)?;
        // ifの条件式には "()" が不要
        if nbytes == 0 {
            // 末尾に "!" が付いているのはマクロ
            debug!("Connection closed.");
            // 文末に ";" を付けないと戻り値になる
            return Ok(());
        }

        // "&" は借用(所有権は渡さず、参照させるようにする)
        print!("{}", str::from_utf8(&buffer[..nbytes])?);

        // 先頭からnbytesまで(nbytesは含まない)のindexで、bufferから取得する
        // (含める場合は、..=nbytes)
        //   https://stackoverflow.com/questions/52932572/what-is-the-dot-dot-equals-operator-in-rust
        //   http://takoyaking.hatenablog.com/entry/2020/01/20/190000
        stream.write_all(&buffer[..nbytes])?;
    }
}
