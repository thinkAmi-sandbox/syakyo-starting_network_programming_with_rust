use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::tcp::{self, MutableTcpPacket, TcpFlags};
use pnet::transport::{
    self, TransportChannelType, TransportProtocol, TransportReceiver, TransportSender,
};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use std::{env, fs, process, thread};
#[macro_use]
extern crate log;

use pnet::datalink::interfaces;


const TCP_SIZE: usize = 20;

struct PacketInfo {
    my_ipaddr: Ipv4Addr,
    target_ipaddr: Ipv4Addr,
    my_port: u16,
    maximum_port: u16,
    scan_type: ScanType,
}

// deriveを使うことにより、CopyやCloneトレイトの機能を提供
// https://doc.rust-jp.rs/rust-by-example-ja/trait/derive.html
// 関数の引数として渡す場合があるため、コピー型にする
// - std::marker::Copyトレイトを実装
// - 移動が行われず、値のコピーが作成される
#[derive(Copy, Clone)]
enum ScanType {
    Syn = TcpFlags::SYN as isize,  // isize = 処理系依存の整数 (usizeは符号なし整数)
    Fin = TcpFlags::FIN as isize,
    // "|" -> ビット演算子(OR), AND演算子は"&"
    Xmas = (TcpFlags::FIN | TcpFlags::URG | TcpFlags::PSH) as isize,
    Null = 0,
}

fn main() {
    // TODO 将来的には設定ファイルではなく、ソースコードで取得したいけど、とりあえず置いておく
    // let interface = print_enable_interfaces();

    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 3{
        error!("Bad number of arguments. [ipaddr] [scantype]");
        process::exit(1);
    }

    // ブロックをまとめて式として扱うことができる
    // https://doc.rust-jp.rs/rust-by-example-ja/expression.html
    // この場合、最後の式(ここではPacketInfo)が、ローカル変数 packet_info に代入される
    let packet_info = {
        let contents = fs::read_to_string(".env").expect("Failed to read env file");
        // Vec<_>について -> "_" には結果の何らかの型が入る
        // https://stackoverflow.com/questions/34363984/what-is-vec
        // イテレータの要素からコレクションを作成する
        // https://qiita.com/lo48576/items/34887794c146042aebf1#collect-iteratort---t%E3%81%AE%E3%82%B3%E3%83%AC%E3%82%AF%E3%82%B7%E3%83%A7%E3%83%B3
        let lines: Vec<_> = contents.split('\n').collect();
        let mut map = HashMap::new();
        for line in lines {
            let elm: Vec<_> = line.split('=').map(str::trim).collect();
            if elm.len() == 2 {
                map.insert(elm[0], elm[1]);
            }
        }

        PacketInfo {
            my_ipaddr: map["MY_IPADDR"].parse().expect("invalid ipaddr"),
            target_ipaddr: args[1].parse().expect("invalid target ipaddr"),
            my_port: map["MY_PORT"].parse().expect("invalid port number"),
            maximum_port: map["MAXIMUM_PORT_NUM"].parse().expect("invalid maximum port num"),
            scan_type: match args[2].as_str() {
                "sS" => ScanType::Syn,
                "sF" => ScanType::Fin,
                "sX" => ScanType::Xmas,
                "sN" => ScanType::Null,
                // ここでの "_" -> その他の処理
                _ => {
                    error!("Undefined scan method, only accept [sS|sF|sN|sX].");
                    process::exit(1);
                }
            }
        }
    };

    // トランスポート層のチャンネルを開く
    // 内部的にはソケット
    let (mut ts, mut tr) = transport::transport_channel(
        1024,
        TransportChannelType::Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Tcp)),
    ).expect("Failed to open channel");


    // 3.4.7
    // パケットの送信と受信を並行で行う
    rayon::join(
        || send_packet(&mut ts, &packet_info),
        || receive_packets(&mut tr, &packet_info),
    );
}

// 3.4.4
// パケットを生成する
// 自分でパケットを作成するのは大変なため、pnetを利用
fn build_packet(packet_info: &PacketInfo) -> [u8; TCP_SIZE] {
    // TCPヘッダの作成
    let mut tcp_buffer = [0u8; TCP_SIZE];
    let mut tcp_header = MutableTcpPacket::new(&mut tcp_buffer[..]).unwrap();
    tcp_header.set_source(packet_info.my_port);

    // オプションを含まないので、20オクテットまでがTCPヘッダ
    // 4オクテット単位で指定する
    tcp_header.set_data_offset(5);
    tcp_header.set_flags(packet_info.scan_type as u16);
    let checksum = tcp::ipv4_checksum(
        &tcp_header.to_immutable(),
        &packet_info.my_ipaddr,
        &packet_info.target_ipaddr,
    );
    tcp_header.set_checksum(checksum);

    tcp_buffer
}

/**
 * 3.4.5
 * TCPヘッダの宛先ポート情報を書き換える
 * チェックサムを計算し直す必要がある
 */
fn reregister_destination_port(
    target: u16,
    tcp_header: &mut MutableTcpPacket,
    packet_info: &PacketInfo
) {
    tcp_header.set_destination(target);
    let checksum = tcp::ipv4_checksum(
        &tcp_header.to_immutable(),
        &packet_info.my_ipaddr,
        &packet_info.target_ipaddr,
    );
    tcp_header.set_checksum(checksum);
}

/**
 * パケットを受信して、スキャン結果を出力する
 */
fn receive_packets(
    tr: &mut TransportReceiver,
    packet_info: &PacketInfo,
) -> Result<(), failure::Error> {
    let mut reply_ports = Vec::new();
    let mut packet_iter = transport::tcp_packet_iter(tr);
    loop {
        // ターゲットからの返信パケット
        let tcp_packet = match packet_iter.next() {
            Ok((tcp_packet, _)) => {
                if tcp_packet.get_destination() == packet_info.my_port {
                    tcp_packet
                } else {
                    continue;
                }
            }
            Err(_) => continue,
        };

        let target_port = tcp_packet.get_source();
        match packet_info.scan_type {
            ScanType::Syn => {
                if tcp_packet.get_flags() == TcpFlags::SYN | TcpFlags::ACK {
                    println!("port {} is open", target_port);
                }
            }

            // SYNスキャン以外は、レスポンスが返ってきたポート (=閉じているポート) を記録
            ScanType::Fin | ScanType::Xmas | ScanType::Null => {
                reply_ports.push(target_port);
            }
        }

        // スキャン対象の最後のポートに対する返信が返ってきたら終了
        if target_port != packet_info.maximum_port {
            continue;
        }
        match packet_info.scan_type {
            ScanType::Fin | ScanType::Xmas | ScanType::Null => {
                for i in 1..=packet_info.maximum_port {
                    if reply_ports.iter().find(|&&x| x == i).is_none() {
                        println!("port {} is open", i);
                    }
                }
            }
            _ => {}
        }
        return Ok(());
    }
}


/**
 * 本文記載なし
 * 指定のレンジにパケットを送信
 */
fn send_packet(
    ts: &mut TransportSender,
    packet_info: &PacketInfo
) -> Result<(), failure::Error> {
    let mut packet = build_packet(packet_info);
    for i in 1..=packet_info.maximum_port {
        let mut tcp_header =
            MutableTcpPacket::new(&mut packet).ok_or_else(|| failure::err_msg("invalid packet"))?;
        reregister_destination_port(i, &mut tcp_header, packet_info);
        thread::sleep(Duration::from_millis(5));
        ts.send_to(tcp_header, IpAddr::V4(packet_info.target_ipaddr))?;
    }
    Ok(())
}


/**
 * 追加
 * 有効なインタフェースポートを確認
 * https://docs.rs/pnet/0.26.0/pnet/datalink/fn.interfaces.html
 */
fn print_enable_interfaces() -> String {
    // Get a vector with all network interfaces found
    let all_interfaces = interfaces();

// Search for the default interface - the one that is
// up, not loopback and has an IP.
    let default_interface = all_interfaces
        .iter()
        .filter(|e| e.is_up() && !e.is_loopback() && e.ips.len() > 0)
        .next();

    match default_interface {
        Some(interface) => {
            println!("Found default interface with [{}].", interface.name);
            // moveを起こさないよう、format!で新しい文字列を生成する
            return format!("{}", interface.name);
        },
        None => {
            println!("Error while finding the default interface.");
            // 戻り値として使えるよう、文字列リテラル(&str)をStringに変換する
            return "".to_string();
        },
    }
}
