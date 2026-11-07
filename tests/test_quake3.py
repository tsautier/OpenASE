import unittest

from openase.quake3 import (
    OUT_OF_BAND_PREFIX,
    DEFAULT_PORT,
    parse_address,
    parse_getservers_response,
    parse_info_response,
    parse_info_string,
    parse_status_response,
)


class Quake3ParserTests(unittest.TestCase):
    def test_parse_address_defaults_to_quake3_port(self):
        self.assertEqual(parse_address("example.org"), ("example.org", DEFAULT_PORT))

    def test_parse_address_accepts_explicit_port(self):
        self.assertEqual(parse_address("example.org:27961"), ("example.org", 27961))

    def test_parse_info_string(self):
        info = parse_info_string(r"\sv_hostname\Test Server\mapname\q3dm17\clients\2")
        self.assertEqual(info["sv_hostname"], "Test Server")
        self.assertEqual(info["mapname"], "q3dm17")
        self.assertEqual(info["clients"], "2")

    def test_parse_info_response(self):
        packet = OUT_OF_BAND_PREFIX + b"infoResponse\n\\hostname\\Q3\\protocol\\68"
        self.assertEqual(parse_info_response(packet)["protocol"], "68")

    def test_parse_status_response(self):
        packet = (
            OUT_OF_BAND_PREFIX
            + b"statusResponse\n"
            + rb"\sv_hostname\Test\sv_maxclients\16\mapname\q3dm6"
            + b'\n12 45 "Player One"\n-3 0 "Bot"\n'
        )

        info, players = parse_status_response(packet)

        self.assertEqual(info["sv_hostname"], "Test")
        self.assertEqual(info["mapname"], "q3dm6")
        self.assertEqual(len(players), 2)
        self.assertEqual(players[0].score, 12)
        self.assertEqual(players[0].ping, 45)
        self.assertEqual(players[0].name, "Player One")

    def test_parse_getservers_response(self):
        packet = (
            OUT_OF_BAND_PREFIX
            + b"getserversResponse"
            + b"\\"
            + bytes([127, 0, 0, 1])
            + (27960).to_bytes(2, "big")
            + b"\\EOT\x00\x00\x00"
        )

        self.assertEqual(parse_getservers_response(packet), ["127.0.0.1:27960"])

    def test_parse_getservers_response_accepts_unpadded_eot(self):
        packet = (
            OUT_OF_BAND_PREFIX
            + b"getserversResponse\\"
            + bytes([10, 0, 0, 2])
            + (27961).to_bytes(2, "big")
            + b"\\EOT"
        )

        self.assertEqual(parse_getservers_response(packet), ["10.0.0.2:27961"])


if __name__ == "__main__":
    unittest.main()
