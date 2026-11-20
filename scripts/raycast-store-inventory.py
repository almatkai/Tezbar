"""Read public Raycast popular pages into a resumable, source-backed inventory.

No account, package installation, repository mutation or build is involved. Cached
pages are immutable observations; use --refresh to collect a new observation.
"""
import argparse
import datetime
import json
import pathlib
import re
import time
import urllib.error
import urllib.request


def fetch(url):
    for attempt in range(4):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "Tezbar-Port-Inventory/1.0"})
            with urllib.request.urlopen(request, timeout=45) as response:
                return response.read().decode("utf-8")
        except (urllib.error.URLError, TimeoutError):
            if attempt == 3:
                raise
            time.sleep(2 ** attempt)


def parse_page(html):
    decoder = json.JSONDecoder()
    found = {}
    for match in re.finditer(r'self\.__next_f\.push\((\[.*?\])\)</script>', html):
        frame = json.loads(match.group(1))
        if len(frame) < 2 or not isinstance(frame[1], str):
            continue
        for entry in re.finditer(r'"extension":', frame[1]):
            try:
                extension = decoder.raw_decode(frame[1][entry.end():])[0]
            except ValueError:
                continue
            if isinstance(extension, dict) and extension.get("store_url"):
                found[extension["id"]] = extension
    # Featured entries are repeated on every page and are not ranked listings.
    # The listing wrapper appears separately from the featured card components.
    listing = html.split('id="list"', 1)
    if len(listing) != 2:
        raise ValueError("Store list anchor missing; inspect changed page format")
    listing_html = listing[1].split('role="navigation"', 1)[0]
    entries = [item for item in found.values() if
               ('href="' + item["store_url"].replace("https://www.raycast.com", "") + '"') in listing_html]
    if not entries:
        raise ValueError("No ranked entries parsed; inspect changed page format")
    links = [int(n) for n in re.findall(r'/store/popular/(\d+)', html)]
    return entries, max(links, default=1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pages", type=int, default=5, help="Maximum pages to fetch; 0 enumerates observed last page")
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("docs/rust-ports-store"))
    args = parser.parse_args()
    if args.pages < 0:
        parser.error("--pages must be nonnegative")
    cache = args.output / "page-cache"
    cache.mkdir(parents=True, exist_ok=True)
    catalog = json.loads(pathlib.Path("crates/tezbar-core/src/extensions/v2_catalog.json").read_text(encoding="utf-8"))
    native = {}
    for entry in catalog:
        for alias in [entry["slug"], *(value.split(".")[-1] for value in entry.get("replaces", []))]:
            native[alias] = entry["id"]
    entries = {}
    last_page = 1
    page = 1
    pages = []
    listing_slots = 0
    while page <= (args.pages or last_page):
        path = cache / (str(page) + ".json")
        if path.exists() and not args.refresh:
            observation = json.loads(path.read_text(encoding="utf-8"))
        else:
            url = "https://www.raycast.com/store/popular" + ("/" + str(page) if page > 1 else "")
            rows, observed_last_page = parse_page(fetch(url))
            observation = {"url": url, "retrievedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                           "observedLastPage": observed_last_page, "entries": rows}
            path.write_text(json.dumps(observation, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
            time.sleep(0.5)
        last_page = max(last_page, observation["observedLastPage"])
        pages.append(page)
        listing_slots += len(observation["entries"])
        for row in observation["entries"]:
            item = dict(row)
            item["observedPage"] = page
            item["nativeCatalogId"] = native.get(item["name"])
            item["portStatus"] = "catalog-mapping-only-not-proof-of-acceptance" if item["nativeCatalogId"] else "no-native-catalog-mapping"
            entries[item["id"]] = item
        print("page", page, "entries", len(observation["entries"]), flush=True)
        page += 1
    result = {"retrievedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(), "pagesObserved": pages,
              "observedLastPage": last_page, "completePageEnumeration": page > last_page,
              "completeStoreEnumeration": page > last_page and listing_slots == len(entries),
              "observedListingSlots": listing_slots,
              "repeatedListingSlots": listing_slots - len(entries),
              "enumerationNote": "Popular pagination can repeat entries with tied download counts; visiting every page does not prove every Store identity was observed.",
              "nativeCatalogEntries": len(catalog), "rankedEntriesObserved": len(entries),
              "entries": sorted(entries.values(), key=lambda item: item["download_count"], reverse=True)}
    (args.output / "popular-inventory.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("observed", len(entries), "ranked entries; last page", last_page)


if __name__ == "__main__":
    main()
