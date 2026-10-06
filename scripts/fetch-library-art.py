#!/usr/bin/env python3
"""Development-only artwork fetcher. Never used by the launcher."""
import json, os, pathlib, unicodedata, urllib.parse, subprocess
root = pathlib.Path(__file__).resolve().parent.parent / "assets/steamgriddb"
root.mkdir(parents=True, exist_ok=True)
key = os.environ["NEBBY_ART_API_KEY"]
def api(path):
    response = subprocess.run(["curl", "-fsS", "--max-time", "30",
        "-H", "Authorization: Bearer " + key, "https://www.steamgriddb.com/api/v2/" + path], capture_output=True)
    if response.returncode: raise RuntimeError("Artwork API request failed; credentials omitted")
    result = json.loads(response.stdout)
    if not result.get("success"): raise RuntimeError("Artwork request rejected")
    return result["data"]
def normal(name):
    return "".join(c for c in unicodedata.normalize("NFKD", name) if not unicodedata.combining(c)).casefold()
games = [("0004000000175E00", "Pokemon Moon"), ("0004000000164800", "Pokemon Sun"),
    ("00040000001B5000", "Pokemon Ultra Sun"), ("00040000001B5100", "Pokemon Ultra Moon"),
    ("0004000000055D00", "Pokemon X"), ("0004000000055E00", "Pokemon Y"),
    ("000400000011C400", "Pokemon Omega Ruby"), ("000400000011C500", "Pokemon Alpha Sapphire")]
records = []
for title, name in games:
    game = next(g for g in api("search/autocomplete/" + urllib.parse.quote(name)) if normal(g["name"]) == normal(name))
    for kind in ["heroes", "grids"]:
        choices = [a for a in api(f"{kind}/game/{game['id']}") if not a.get("nsfw") and not a.get("humor") and a.get("mime") in ("image/png", "image/jpeg")]
        if not choices: continue
        art = max(choices, key=lambda a: a.get("score", 0))
        extension = ".png" if art["mime"] == "image/png" else ".jpg"
        filename = title + "-" + kind + extension
        subprocess.run(["curl", "-fsSL", "--max-time", "45", art["url"], "-o", str(root / filename)], check=True)
        records.append({"title_id": title, "game": game["name"], "filename": filename,
            "asset_id": art["id"], "url": art["url"], "author": art.get("author"),
            "rights": "SteamGridDB community artwork; redistribution rights not established. Local use only."})
    print("ART_FETCHED", name, flush=True)
(root / "PROVENANCE.json").write_text(json.dumps(records, indent=2) + "\n")
