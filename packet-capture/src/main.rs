use pnet::datalink;
use pnet::datalink::Channel::Ethernet;
use pnet::packet::ethernet::{EtherTypes, EthernetPacket};
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::ipv6::Ipv6Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::udp::UdpPacket;
use pnet::packet::Packet;
#[macro_use]
extern crate log;

use std::env;

mod packets;
use packets::GettableEndPoints;

const WIDTH:usize = 20;

fn main() {
    // 環境変数の設定
    // https://doc.rust-lang.org/std/env/fn.set_var.html
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // env::args().collect : コマンドライン引数のイテレータを返し、collect()関数を実行
    //     https://doc.rust-jp.rs/book/second-edition/ch12-01-accepting-command-line-arguments.html
    //   collect()関数 : イテレータをコレクションにする
    //     https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
    // Vec<T> 配列を表現する型 (配列と異なり、要素の追加削除・ヒープ領域に置かれる(配列はスタック領域)
    //   スタック：関数の引数やローカル変数など。確保と解放が速い。サイズ小さい。
    //   ヒープ：Vec<T>など。複雑な仕組みで管理。
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        error!("Please specify target interface name");
        // exitコードを伴って、プロセスを終了する
        //   https://doc.rust-lang.org/std/process/fn.exit.html
        std::process::exit(1);
    }

    let interface_name = &args[1];

    // インタフェースの選択
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        // *interface_name と先頭にアスタリスクをつけることで、参照(ポインタ)の値を取得できる
        //   https://stackoverflow.com/questions/40531912/what-is-the-usage-of-the-asterisk-symbol-in-rust
        .find(|iface| iface.name == *interface_name)
        // unwrap時にエラーメッセージを表示する
        //   https://3c1u.hatenablog.com/entry/2019/09/18/060000
        .expect("Failed to get interface");

    // データリンクのチャンネルを取得
    let (_tx, mut rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!("Failed to create datalink channel {}", e),
    };

    loop {
        match rx.next() {
            Ok(frame) => {
                // 受信データからイーサネットフレームを構築
                let frame = EthernetPacket::new(frame).unwrap();
                match frame.get_ethertype() {
                    EtherTypes::Ipv4 => {
                        ipv4_handler(&frame);
                    }
                    EtherTypes::Ipv6 => {
                        ipv6_handler(&frame);
                    }
                    _ => {
                        info!("Not an IPv4 or IPv6 packet");
                    }
                }
            }
            Err(e) => {
                error!("Failed to read: {}", e);
            }
        }
    }
}

/**
 * IPv4パケットを構築し、次のレイヤのハンドラを呼び出す
 */
fn ipv4_handler(ethernet: &EthernetPacket) {
    // Someは列挙型
    //   https://cha-shu00.hatenablog.com/entry/2019/03/06/220546
    // ここでは if let を使っている
    //   https://doc.rust-jp.rs/book/second-edition/ch06-03-if-let.html
    //   これにより、判定とpacket変数への代入を一度に行っている(Pythonの代入式のようなもの?)
    if let Some(packet) = Ipv4Packet::new(ethernet.payload()) {
        // 成功した場合(packetが取得できた場合)
        match packet.get_next_level_protocol() {
            IpNextHeaderProtocols::Tcp => {
                tcp_handler(&packet);
            }
            IpNextHeaderProtocols::Udp => {
                udp_handler(&packet);
            }
            _ => {
                info!("Not TCP or UDP packet");
            }
        }
    }
}

/**
 * IPv6パケットを構築し、次のレイヤのハンドラを呼び出す
 */
fn ipv6_handler(ethernet: &EthernetPacket) {
    if let Some(packet) = Ipv6Packet::new(ethernet.payload()) {
        match packet.get_next_header() {
            IpNextHeaderProtocols::Tcp => {
                tcp_handler(&packet);
            }
            IpNextHeaderProtocols::Udp => {
                udp_handler(&packet);
            }
            _ => {
                info!("Not TCP or UDP packet");
            }
        }
    }
}

/**
 * TCPパケットを構築する
 */
// &GettableEndPoints では、コンパイル時にwarningが出たので差し替えた
//   warning: trait objects without an explicit `dyn` are deprecated
//   help: use `dyn`: `dyn GettableEndPoints`
// dynの入れ方はカッコを使った
//   https://qnighy.hatenablog.com/entry/2018/01/28/220000
fn tcp_handler(packet: &(dyn GettableEndPoints)) {
    let tcp = TcpPacket::new(packet.get_payload());
    if let Some(tcp) = tcp {
        print_packet_info(packet, &tcp, "TCP");
    }
}

/**
 * UDPパケットを構築する
 */
fn udp_handler(packet: &(dyn GettableEndPoints)) {
    let udp = UdpPacket::new(packet.get_payload());
    if let Some(udp) = udp {
        print_packet_info(packet, &udp, "UDP");
    }
}

/**
 * アプリケーション層のデータをバイナリで表示する
 */
fn print_packet_info(l3: &(dyn GettableEndPoints), l4: &(dyn GettableEndPoints), proto: &str) {
    println!(
        "Captured a {} packet from {}|{} ti {}|{} \n",
        proto,
        l3.get_source(),
        l4.get_source(),
        l3.get_destination(),
        l4.get_destination()
    );

    let payload = l4.get_payload();
    let len = payload.len();

    // ペイロードの表示
    // 指定した定数幅で表示を行う
    for i in 0..len {
        print!("{:<02X}", payload[i]);

        if i % WIDTH == WIDTH - 1 || i == len - 1 {
            for _j in 0..WIDTH - 1 - (i % (WIDTH)) {
                print!(" ");
            }
            print!("| ");

            for j in i - i % WIDTH..=i {
                if payload[j].is_ascii_alphabetic() {
                    print!("{}", payload[j] as char);
                }
                else {
                    // 非ascii文字は "." で表示
                    print!(".");
                }
            }
            println!();
        }
    }
    println!("{}", "=".repeat(WIDTH * 3));
    println!();
}
