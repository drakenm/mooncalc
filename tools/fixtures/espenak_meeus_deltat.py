#!/usr/bin/env python3
"""Evaluate the Espenak & Meeus (2006) Delta T polynomials at check epochs.

Source of the expressions: F. Espenak, "Polynomial Expressions for Delta T",
NASA Eclipse Web Site, adapted from the Five Millennium Canon of Solar Eclipses
(Espenak & Meeus 2006), https://eclipse.gsfc.nasa.gov/SEhelp/deltatpoly2004.html
(retrieved 2026-11-11). The values written are COMPUTED here from those
published expressions; they are not independently published numbers. They pin
an implementation of the same piecewise polynomial, not the true Delta T.

    python -I tools/fixtures/espenak_meeus_deltat.py > tests/fixtures/deltat/espenak_meeus_polynomial_checks.csv

The input is the decimal year y (Espenak: y = year + (month - 0.5)/12). The
long-term parabola is evaluated with u = (y - 1820)/100. The lunar
secular-acceleration correction c = -0.000012932 (y - 1955)^2, which the page
says must be added for use with ELP-2000/82 outside 1955-2005, is given in a
separate column.
"""
import sys


def delta_t(y):
    if y < -500:
        u = (y - 1820) / 100
        return -20 + 32 * u * u
    if y < 500:
        u = y / 100
        return (10583.6 - 1014.41 * u + 33.78311 * u**2 - 5.952053 * u**3
                - 0.1798452 * u**4 + 0.022174192 * u**5 + 0.0090316521 * u**6)
    if y < 1600:
        u = (y - 1000) / 100
        return (1574.2 - 556.01 * u + 71.23472 * u**2 + 0.319781 * u**3
                - 0.8503463 * u**4 - 0.005050998 * u**5 + 0.0083572073 * u**6)
    if y < 1700:
        t = y - 1600
        return 120 - 0.9808 * t - 0.01532 * t**2 + t**3 / 7129
    if y < 1800:
        t = y - 1700
        return 8.83 + 0.1603 * t - 0.0059285 * t**2 + 0.00013336 * t**3 - t**4 / 1174000
    if y < 1860:
        t = y - 1800
        return (13.72 - 0.332447 * t + 0.0068612 * t**2 + 0.0041116 * t**3 - 0.00037436 * t**4
                + 0.0000121272 * t**5 - 0.0000001699 * t**6 + 0.000000000875 * t**7)
    if y < 1900:
        t = y - 1860
        return 7.62 + 0.5737 * t - 0.251754 * t**2 + 0.01680668 * t**3 - 0.0004473624 * t**4 + t**5 / 233174
    if y < 1920:
        t = y - 1900
        return -2.79 + 1.494119 * t - 0.0598939 * t**2 + 0.0061966 * t**3 - 0.000197 * t**4
    if y < 1941:
        t = y - 1920
        return 21.20 + 0.84493 * t - 0.076100 * t**2 + 0.0020936 * t**3
    if y < 1961:
        t = y - 1950
        return 29.07 + 0.407 * t - t**2 / 233 + t**3 / 2547
    if y < 1986:
        t = y - 1975
        return 45.45 + 1.067 * t - t**2 / 260 - t**3 / 718
    if y < 2005:
        t = y - 2000
        return (63.86 + 0.3345 * t - 0.060374 * t**2 + 0.0017275 * t**3 + 0.000651814 * t**4
                + 0.00002373599 * t**5)
    if y < 2050:
        t = y - 2000
        return 62.92 + 0.32217 * t + 0.005589 * t**2
    if y < 2150:
        return -20 + 32 * ((y - 1820) / 100) ** 2 - 0.5628 * (2150 - y)
    u = (y - 1820) / 100
    return -20 + 32 * u * u


EPOCHS = [-1000.0, -500.0, -250.0, 0.0, 250.0, 500.0, 750.0, 1000.0, 1250.0, 1500.0,
          1600.0, 1650.0, 1700.0, 1750.0, 1800.0, 1830.0, 1860.0, 1880.0, 1900.0, 1910.0,
          1920.0, 1930.0, 1941.0, 1950.0, 1961.0, 1970.0, 1980.0, 1986.0, 1990.0, 2000.0,
          2004.5, 2005.0, 2010.0, 2020.0, 2030.0, 2049.5, 2050.0, 2100.0, 2149.5, 2150.0,
          2200.0, 3000.0]


def main():
    sys.stdout.reconfigure(newline="\n")  # LF output on Windows too
    out = sys.stdout
    out.write("year,delta_t_s,lunar_correction_c_s\n")
    for y in EPOCHS:
        c = 0.0 if 1955 <= y <= 2005 else -0.000012932 * (y - 1955) ** 2
        out.write(f"{y:.1f},{delta_t(y):.4f},{c:.4f}\n")


if __name__ == "__main__":
    main()
