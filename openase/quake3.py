"""Quake 3 server query support.

Quake 3 connectionless packets start with four 0xFF bytes followed by an ASCII
command. OpenASE currently uses the documented `getinfo` and `getstatus`
commands and intentionally sends short challenge strings to avoid oversized
queries.
"""

from __future__ import annotations

import re
import shlex
import socket
from dataclasses import dataclass, field

DEFAULT_PORT = 27960
DEFAULT_MASTER = "master.ioquake3.org"
DEFAULT_MASTER_PORT = 27950
OUT_OF_BAND_PREFIX = b"\xff\xff\xff\xff"
CHALLENGE = "openase"


@dataclass(slots=True)
class Quake3Player:
    """One player row from a Quake 3 `statusResponse` packet."""

    score: int
    ping: int
    name: str


@dataclass(slots=True)
class Quake3Server:
    """Parsed Quake 3 server response."""

    host: str
    port: int
    online: bool
    info: dict[str, str] = field(default_factory=dict)
    players: list[Quake3Player] = field(default_factory=list)
    error: str | None = None

    @property
    def address(self) -> str:
        return f"{self.host}:{self.port}"


class Quake3Client:
    """UDP client for Quake 3 server status queries."""

    def __init__(self, timeout: float = 2.0) -> None:
        self.timeout = timeout

    def status(self, host: str, port: int = DEFAULT_PORT) -> Quake3Server:
        """Return status for one Quake 3 server.

        A failed UDP query returns an offline `Quake3Server` instead of raising,
        which lets scanners keep processing the rest of a server list.
        """

        try:
            packet = self._query(host, port, f"getstatus {CHALLENGE}")
            info, players = parse_status_response(packet)
            return Quake3Server(host=host, port=port, online=True, info=info, players=players)
        except OSError as exc:
            return Quake3Server(host=host, port=port, online=False, error=str(exc))
        except ValueError as exc:
            return Quake3Server(host=host, port=port, online=False, error=str(exc))

    def info(self, host: str, port: int = DEFAULT_PORT) -> Quake3Server:
        """Return lightweight server information from `infoResponse`."""

        try:
            packet = self._query(host, port, f"getinfo {CHALLENGE}")
            info = parse_info_response(packet)
            return Quake3Server(host=host, port=port, online=True, info=info)
        except OSError as exc:
            return Quake3Server(host=host, port=port, online=False, error=str(exc))
        except ValueError as exc:
            return Quake3Server(host=host, port=port, online=False, error=str(exc))

    def master_servers(
        self,
        host: str = DEFAULT_MASTER,
        port: int = DEFAULT_MASTER_PORT,
        protocol: int = 68,
    ) -> list[str]:
        """Return `host:port` entries advertised by a Quake 3 master server."""

        packet = self._query(host, port, f"getservers {protocol} empty full\n")
        return parse_getservers_response(packet)

    def _query(self, host: str, port: int, command: str) -> bytes:
        payload = OUT_OF_BAND_PREFIX + command.encode("ascii")
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.settimeout(self.timeout)
            sock.sendto(payload, (host, port))
            data, _ = sock.recvfrom(8192)
        return data


def parse_address(address: str, default_port: int = DEFAULT_PORT) -> tuple[str, int]:
    """Parse `host` or `host:port` into a host and UDP port."""

    if address.startswith("["):
        raise ValueError("IPv6 bracket notation is not supported yet")

    host, separator, raw_port = address.rpartition(":")
    if not separator:
        return address, default_port
    if not host:
        raise ValueError("missing host")
    return host, int(raw_port)


def parse_info_response(packet: bytes) -> dict[str, str]:
    text = decode_connectionless(packet, "infoResponse")
    return parse_info_string(text.strip())


def parse_status_response(packet: bytes) -> tuple[dict[str, str], list[Quake3Player]]:
    text = decode_connectionless(packet, "statusResponse")
    lines = [line for line in text.splitlines() if line]
    if not lines:
        raise ValueError("statusResponse did not contain an info string")

    info = parse_info_string(lines[0])
    players = [parse_player_line(line) for line in lines[1:]]
    return info, players


def parse_getservers_response(packet: bytes) -> list[str]:
    """Parse a Quake 3 `getserversResponse` packet from a master server."""

    header = OUT_OF_BAND_PREFIX + b"getserversResponse"
    if not packet.startswith(header):
        raise ValueError("response is not a Quake 3 getserversResponse packet")

    body = packet[len(header) :]
    addresses: list[str] = []
    index = 0
    while index < len(body):
        marker = body[index : index + 1]
        if marker != b"\\":
            index += 1
            continue

        chunk = body[index + 1 : index + 7]
        if chunk == b"EOT\x00\x00\x00":
            break
        if len(chunk) < 6:
            raise ValueError("truncated getserversResponse address entry")

        ip_octets = chunk[:4]
        port = int.from_bytes(chunk[4:6], byteorder="big")
        host = ".".join(str(octet) for octet in ip_octets)
        addresses.append(f"{host}:{port}")
        index += 7

    return addresses


def decode_connectionless(packet: bytes, expected_command: str) -> str:
    """Validate the packet prefix and command, then return the response body."""

    if not packet.startswith(OUT_OF_BAND_PREFIX):
        raise ValueError("response is missing the Quake 3 out-of-band prefix")

    text = packet[len(OUT_OF_BAND_PREFIX) :].decode("latin-1", errors="replace")
    command, separator, body = text.partition("\n")
    if command.strip() != expected_command:
        raise ValueError(f"expected {expected_command}, got {command.strip() or 'empty response'}")
    if not separator:
        raise ValueError(f"{expected_command} response is missing a body")
    return body


def parse_info_string(raw: str) -> dict[str, str]:
    """Parse a Quake 3 backslash-delimited info string into key/value pairs."""

    if not raw:
        return {}
    parts = raw.split("\\")
    if parts and parts[0] == "":
        parts = parts[1:]
    if len(parts) % 2 != 0:
        raise ValueError("info string has an odd number of fields")

    result: dict[str, str] = {}
    for index in range(0, len(parts), 2):
        key = parts[index]
        value = parts[index + 1]
        if key:
            result[key] = value
    return result


def parse_player_line(line: str) -> Quake3Player:
    """Parse a status player row: score, ping, and quoted name."""

    tokens = shlex.split(line, posix=False)
    if len(tokens) < 3:
        raise ValueError(f"invalid player row: {line}")

    score = int(tokens[0])
    ping = int(tokens[1])
    name = " ".join(tokens[2:])
    name = re.sub(r'^"(.*)"$', r"\1", name)
    return Quake3Player(score=score, ping=ping, name=name)
