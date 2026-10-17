#!/usr/bin/env python3
"""HOME-029: data/places/places.tsv.gz, the places Numa Home names a trip by.

From GeoNames (https://www.geonames.org, CC BY 4.0): cities1000.txt (every
place of 1 000 people or more), countryInfo.txt and admin1CodesASCII.txt,
from https://download.geonames.org/export/dump/. One line a place, most
people first: name, latitude, longitude (three decimals, about 100 m),
region, country, population.

    python3 -I dev/places.py cities1000.txt countryInfo.txt admin1CodesASCII.txt data/places/places.tsv.gz
"""
import gzip
import sys

cities, countries_file, admin_file, out = sys.argv[1:5]

countries = {}
for line in open(countries_file, encoding="utf-8"):
    if line.startswith("#"):
        continue
    parts = line.rstrip("\n").split("\t")
    countries[parts[0]] = parts[4]

regions = {}
for line in open(admin_file, encoding="utf-8"):
    code, name, *_ = line.rstrip("\n").split("\t")
    regions[code] = name

rows = []
for line in open(cities, encoding="utf-8"):
    p = line.rstrip("\n").split("\t")
    name, lat, lon, cc, admin, population = p[1], float(p[4]), float(p[5]), p[8], p[10], int(p[14] or 0)
    if cc not in countries:
        continue
    rows.append((population, name, lat, lon, regions.get(f"{cc}.{admin}", ""), countries[cc]))
rows.sort(key=lambda row: -row[0])

with gzip.open(out, "wt", encoding="utf-8", compresslevel=9) as file:
    for population, name, lat, lon, region, country in rows:
        file.write(f"{name}\t{lat:.3f}\t{lon:.3f}\t{region}\t{country}\t{population}\n")
print(f"{len(rows)} places in {out}")
