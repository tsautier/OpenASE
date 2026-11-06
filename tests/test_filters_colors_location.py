import tempfile
import unittest
from pathlib import Path

from openase.ase_filters import parse_filter_paths, parse_game_definitions
from openase.colors import first_quake_color, quake_color_segments, strip_quake_colors
from openase.location import UNKNOWN_LOCATION, load_countries, server_location


class FiltersColorsLocationTests(unittest.TestCase):
    def test_parse_filter_paths_keeps_tree_lines(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "filters.txt"
            path.write_text(
                "\n".join(
                    [
                        "// comment",
                        "Quake 3 Arena",
                        "Half-Life 2: Source Mods",
                        "ServerType: Q3",
                        "1 if servertype != \"Q3\" remove",
                        "Quake 3 Arena\\baseq3",
                        "Mutex: Q3",
                    ]
                ),
                encoding="latin-1",
            )

            self.assertEqual(
                parse_filter_paths(path), ["Quake 3 Arena", "Half-Life 2: Source Mods", "Quake 3 Arena\\baseq3"]
            )

    def test_parse_game_definitions_excludes_modifiers(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "filters.txt"
            path.write_text(
                "\n".join(
                    [
                        "Quake 3 Arena",
                        "ServerType: Q3",
                        "ExeName: quake3.exe",
                        "Quake 3 Arena\\baseq3",
                        "Modifiers\\Ping",
                    ]
                ),
                encoding="latin-1",
            )

            games = parse_game_definitions(path)

            self.assertIn("Quake 3 Arena", games)
            self.assertNotIn("Modifiers", games)
            self.assertEqual(games["Quake 3 Arena"].metadata["ExeName"], "quake3.exe")

    def test_strip_quake_colors(self):
        self.assertEqual(strip_quake_colors("^1Red^7 Name"), "Red Name")
        self.assertEqual(strip_quake_colors("^11"), "1")
        self.assertEqual(strip_quake_colors("^xff0000Red ^NReset"), "Red Reset")
        self.assertEqual(strip_quake_colors("^Bblink^b"), "blink")

    def test_first_quake_color(self):
        self.assertEqual(first_quake_color("^4Blue"), "#0000ff")
        self.assertEqual(first_quake_color("^xff0000Red"), "#ff0000")
        self.assertEqual(first_quake_color("^B^F^2Green"), "#00ff00")

    def test_quake_color_segments(self):
        segments = quake_color_segments("^1Red ^2Green ^xff0000Hex")

        self.assertEqual([(segment.text, segment.color) for segment in segments], [
            ("Red ", "#ff0000"),
            ("Green ", "#00ff00"),
            ("Hex", "#ff0000"),
        ])

    def test_quake_color_segments_keeps_literal_after_color_code(self):
        segments = quake_color_segments("^11")

        self.assertEqual([(segment.text, segment.color) for segment in segments], [("1", "#ff0000")])

    def test_load_countries(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "countries.txt"
            path.write_text("FR        France\nDE        Germany\n", encoding="latin-1")

            countries = load_countries(path)

            self.assertEqual(countries[0].code, "FR")
            self.assertEqual(countries[0].name, "France")

    def test_server_location_does_not_guess(self):
        self.assertEqual(server_location({}), UNKNOWN_LOCATION)
        self.assertEqual(server_location({"country": "France"}), "France")


if __name__ == "__main__":
    unittest.main()
