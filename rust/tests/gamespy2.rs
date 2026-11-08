//! Offline tests for GameSpy 2/3 queries and the server browsing master.

use openase::gamespy_sb::{GoaCrypt, cipher_key, list_request, parse_list, split_header};
use openase::gamespy2::{
    REQUEST_ID, V3Packet, is_complete, merge_v3, parse_challenge, parse_v2, parse_v3_packet,
    server_from_response, v2_request, v3_challenge_request, v3_request,
};

/// Halo: Combat Evolved answer, shaped like a live capture (names changed).
const HALO_ANSWER: &[u8] = b"\x00\x04\x05\x06\x07\
hostname\x00 BG Halo Classic\x00gamever\x0001.00.10.0621\x00hostport\x002325\x00\
maxplayers\x0016\x00password\x000\x00mapname\x00bloodgulch\x00dedicated\x001\x00\
numplayers\x003\x00gametype\x00CTF\x00teamplay\x001\x00gamevariant\x00Halo_Classic\x00\
fraglimit\x0015\x00\x00\
\x00\x03player_\x00score_\x00ping_\x00team_\x00\x00\
Alpha\x000\x0048\x001\x00Bravo\x002\x00217\x000\x00Charlie\x001\x0020\x001\x00\
\x00\x02team_t\x00score_t\x00\x00Red\x0012\x00Blue\x008\x00";

#[test]
fn requests_have_the_documented_layout() {
    assert_eq!(
        v2_request(),
        [0xfe, 0xfd, 0x00, 4, 5, 6, 7, 0xff, 0xff, 0xff]
    );
    assert_eq!(v3_challenge_request(), [0xfe, 0xfd, 0x09, 4, 5, 6, 7]);
    assert_eq!(
        v3_request(None),
        [0xfe, 0xfd, 0x00, 4, 5, 6, 7, 0xff, 0xff, 0xff, 0x01]
    );
    assert_eq!(
        v3_request(Some([1, 2, 3, 4])),
        [
            0xfe, 0xfd, 0x00, 4, 5, 6, 7, 1, 2, 3, 4, 0xff, 0xff, 0xff, 0x01
        ]
    );
}

#[test]
fn gamespy2_halo_answer_is_parsed() {
    let response = parse_v2(HALO_ANSWER).unwrap();
    assert_eq!(response.rules.len(), 12);
    assert_eq!(response.players.len(), 3);
    assert_eq!(response.players[1]["player"], "Bravo");
    assert_eq!(response.players[1]["ping"], "217");
    assert_eq!(response.teams.len(), 2);
    assert_eq!(response.teams[1]["team_t"], "Blue");

    let server = server_from_response("halo", "1.2.3.4", 2325, response);
    assert!(server.online);
    assert_eq!(server.name, " BG Halo Classic");
    assert_eq!(server.map, "bloodgulch");
    assert_eq!(server.game_type, "CTF");
    assert_eq!(server.num_players, 3);
    assert_eq!(server.max_players, Some(16));
    assert!(!server.password);
    assert_eq!(server.join_port, 2325);
    assert_eq!(server.players[1].score, 2);
    assert_eq!(server.players[2].ping, Some(20));
    assert_eq!(server.rule("team_t0"), Some("Red"));
    assert_eq!(server.rule("score_t1"), Some("8"));
}

#[test]
fn gamespy2_rejects_foreign_packets() {
    assert!(parse_v2(b"\\hostname\\x\\final\\").is_err());
    let mut wrong_id = HALO_ANSWER.to_vec();
    wrong_id[1] = 9;
    assert!(parse_v2(&wrong_id).is_err());
}

#[test]
fn gamespy3_challenge_is_sent_as_big_endian_number() {
    let mut packet = vec![0x09];
    packet.extend_from_slice(&REQUEST_ID);
    packet.extend_from_slice(b"-1234567\0");
    assert_eq!(
        parse_challenge(&packet),
        Some((-1234567i32 as u32).to_be_bytes())
    );
    let mut packet = vec![0x09];
    packet.extend_from_slice(&REQUEST_ID);
    packet.extend_from_slice(b"8765\0");
    assert_eq!(parse_challenge(&packet), Some(8765u32.to_be_bytes()));
    assert_eq!(parse_challenge(b"\x00\x04\x05\x06\x07"), None);
}

fn v3_packet(index: u8, data: &[u8]) -> Vec<u8> {
    let mut packet = vec![0x00];
    packet.extend_from_slice(&REQUEST_ID);
    packet.extend_from_slice(b"splitnum\0");
    packet.push(index);
    packet.extend_from_slice(data);
    packet
}

#[test]
fn gamespy3_split_battlefield2_answer_is_merged() {
    // Packet 0: rules, then the first two players.
    let first = v3_packet(
        0x00,
        b"\x00hostname\x00=DOG= Karkand\x00gamename\x00battlefield2\x00gamever\x001.5.3153-802.0\x00\
mapname\x00Strike At Karkand\x00gametype\x00gpm_cq\x00gamevariant\x00bf2\x00numplayers\x003\x00\
maxplayers\x0064\x00hostport\x0016567\x00password\x001\x00bf2_ranked\x001\x00\x00\
\x01player_\x00\x00 Alpha\x00 Bravo\x00\x00score_\x00\x0012\x007\x00\x00ping_\x00\x0045\x0060\x00\x00\
AIBot_\x00\x000\x000\x00\x00\x00",
    );
    // Packet 1 (last): the third player continues at offset 2, then teams.
    let last = v3_packet(
        0x81,
        b"\x01player_\x00\x02 Charlie\x00\x00score_\x00\x023\x00\x00ping_\x00\x020\x00\x00\
AIBot_\x00\x021\x00\x00\x00\
\x02team_t\x00\x00MEC\x00USMC\x00\x00score_t\x00\x000\x000\x00\x00\x00",
    );

    let packets = [
        parse_v3_packet(&last).unwrap(),
        parse_v3_packet(&first).unwrap(),
    ];
    assert_eq!(packets[0].index, 1);
    assert!(packets[0].last);
    assert!(is_complete(&packets));
    assert!(!is_complete(&packets[..1]));

    let response = merge_v3(&packets);
    assert_eq!(response.players.len(), 3);
    assert_eq!(response.players[2]["player"], " Charlie");
    assert_eq!(response.teams.len(), 2);

    let server = server_from_response("bf2", "9.8.7.6", 29900, response);
    assert_eq!(server.name, "=DOG= Karkand");
    assert_eq!(server.map, "Strike At Karkand");
    assert_eq!(server.game_type, "gpm_cq");
    assert_eq!(server.num_players, 3);
    assert_eq!(server.max_players, Some(64));
    assert!(server.password);
    assert_eq!(server.port, 29900);
    assert_eq!(server.join_port, 16567);
    assert_eq!(server.bots, 1);
    assert_eq!(server.players[0].score, 12);
    assert_eq!(server.players[1].ping, Some(60));
    assert_eq!(server.rule("team_t1"), Some("USMC"));
    assert_eq!(server.rule("bf2_ranked"), Some("1"));
}

#[test]
fn gamespy3_packet_without_splitnum_is_single() {
    let packet = b"\x00\x04\x05\x06\x07\x00hostname\x00x\x00\x00";
    let packet = parse_v3_packet(packet).unwrap();
    assert_eq!(
        packet,
        V3Packet {
            index: 0,
            last: true,
            data: b"\x00hostname\x00x\x00\x00".to_vec()
        }
    );
}

// --- server browsing master ---

#[test]
fn list_request_has_length_prefix_and_fields() {
    let request = list_request("battlefield2", b"ABCDEFGH", "\\hostname");
    assert_eq!(
        usize::from(u16::from_be_bytes([request[0], request[1]])),
        request.len()
    );
    assert_eq!(&request[2..5], &[0x00, 0x01, 0x03]);
    let mut expected = b"battlefield2\0battlefield2\0ABCDEFGH\0\\hostname\0".to_vec();
    expected.extend_from_slice(&[0, 0, 0, 0]);
    assert_eq!(&request[9..], expected.as_slice());
}

/// A decrypted list: 3 servers with every optional part, then the end marker.
fn plain_list() -> Vec<u8> {
    let mut list = vec![81, 64, 10, 20]; // our public IP
    list.extend_from_slice(&29900u16.to_be_bytes());
    list.push(1); // one field
    list.push(0); // string type
    list.extend_from_slice(b"hostname\0");
    list.push(1); // one popular value
    list.extend_from_slice(b"Popular\0");
    // Server 1: own port, inline key value.
    list.push(0x40 | 0x10);
    list.extend_from_slice(&[1, 2, 3, 4]);
    list.extend_from_slice(&29901u16.to_be_bytes());
    list.push(0xff);
    list.extend_from_slice(b"First\0");
    // Server 2: default port, popular value index.
    list.push(0x40);
    list.extend_from_slice(&[5, 6, 7, 8]);
    list.push(0);
    // Server 3: private IP and port, ICMP IP, full rules.
    list.push(0x02 | 0x20 | 0x08 | 0x80 | 0x01);
    list.extend_from_slice(&[9, 10, 11, 12]);
    list.extend_from_slice(&[192, 168, 0, 2]);
    list.extend_from_slice(&29910u16.to_be_bytes());
    list.extend_from_slice(&[9, 10, 11, 13]);
    list.extend_from_slice(b"hostname\0Third\0numplayers\x004\0\0");
    // End marker.
    list.push(0);
    list.extend_from_slice(&[0xff; 4]);
    list
}

#[test]
fn server_list_is_parsed_and_needs_the_end_marker() {
    let list = plain_list();
    assert_eq!(
        parse_list(&list),
        Some(vec![
            "1.2.3.4:29901".to_string(),
            "5.6.7.8:29900".to_string(),
            "9.10.11.12:29900".to_string(),
        ])
    );
    assert_eq!(parse_list(&list[..list.len() - 3]), None);
    assert_eq!(parse_list(&list[..20]), None);
}

#[test]
fn encrypted_list_round_trips_through_the_master_cipher() {
    let secret = b"hW6m9a";
    let client_challenge = *b"q7Tz!x2K";
    let crypt_challenge = [3u8; 10];
    let server_challenge: Vec<u8> = (0..25u8).map(|i| i.wrapping_mul(37) ^ 0x5a).collect();

    // What the master sends: clear header, then the encrypted list.
    let mut answer = vec![10 ^ 0xec];
    answer.extend_from_slice(&crypt_challenge);
    answer.push(25 ^ 0xea);
    answer.extend_from_slice(&server_challenge);
    let mut body = plain_list();
    let key = cipher_key(secret, &client_challenge, &server_challenge);
    GoaCrypt::new(&key).encrypt(&mut body);
    assert_ne!(body, plain_list());
    answer.extend_from_slice(&body);

    // What the client does, decrypting in two chunks like a TCP stream.
    let (header_len, challenge) = split_header(&answer).unwrap();
    assert_eq!(header_len, 37);
    assert_eq!(challenge, server_challenge.as_slice());
    let mut cipher = GoaCrypt::new(&cipher_key(secret, &client_challenge, challenge));
    let mut data = answer[header_len..].to_vec();
    let (head, tail) = data.split_at_mut(13);
    cipher.decrypt(head);
    cipher.decrypt(tail);
    assert_eq!(data, plain_list());
    assert_eq!(parse_list(&data).map(|list| list.len()), Some(3));

    // A wrong game key gives garbage.
    let mut wrong = answer[header_len..].to_vec();
    GoaCrypt::new(&cipher_key(b"xxxxxx", &client_challenge, challenge)).decrypt(&mut wrong);
    assert_ne!(wrong, plain_list());
}

#[test]
fn header_is_incomplete_until_the_server_challenge_arrived() {
    assert_eq!(split_header(&[]), None);
    let mut answer = vec![10 ^ 0xec];
    answer.extend_from_slice(&[0; 10]);
    assert_eq!(split_header(&answer), None);
    answer.push(25 ^ 0xea);
    answer.extend_from_slice(&[1; 24]);
    assert_eq!(split_header(&answer), None);
    answer.push(1);
    assert_eq!(split_header(&answer).map(|(len, _)| len), Some(37));
}

#[test]
fn cipher_key_mixes_secret_and_challenges() {
    let challenge = *b"ABCDEFGH";
    assert_eq!(cipher_key(b"key", &challenge, &[]), challenge);
    let mixed = cipher_key(b"hW6m9a", &challenge, &[1, 2, 3]);
    assert_ne!(mixed, challenge);
    // i = 0 always targets byte 0: key[0] ^= key[0] ^ 1.
    assert_eq!(mixed[0], 1);
}
