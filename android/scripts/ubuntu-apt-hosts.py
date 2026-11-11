"""Resolve only the official Ubuntu mirrors for an ephemeral CI runner."""
import ipaddress
import json
import subprocess
import urllib.parse
import urllib.request


def public_ipv4(values):
    addresses = []
    for value in values:
        address = ipaddress.ip_address(value)
        if address.version != 4 or not address.is_global:
            raise ValueError("Ubuntu mirror resolution must return public IPv4 addresses")
        if str(address) not in addresses:
            addresses.append(str(address))
    if not addresses:
        raise ValueError("Ubuntu mirror resolution returned no IPv4 addresses")
    return addresses


def resolve(host):
    try:
        result = subprocess.run(["getent", "ahostsv4", host], check=True,
                                capture_output=True, text=True, timeout=8)
        return public_ipv4(line.split()[0] for line in result.stdout.splitlines())
    except (subprocess.SubprocessError, ValueError):
        pass
    errors = []
    for resolver in ("https://cloudflare-dns.com/dns-query", "https://dns.google/resolve"):
        try:
            query = urllib.parse.urlencode({"name": host, "type": "A"})
            request = urllib.request.Request(resolver + "?" + query,
                                             headers={"Accept": "application/dns-json"})
            with urllib.request.urlopen(request, timeout=10) as response:
                data = json.load(response)
            if data.get("Status") != 0:
                raise ValueError("HTTPS resolver did not return a successful response")
            return public_ipv4(answer["data"] for answer in data.get("Answer", [])
                               if answer.get("type") == 1
                               and answer.get("name", "").rstrip(".") == host)
        except (OSError, ValueError, KeyError) as error:
            errors.append(str(error))
    raise RuntimeError(f"Unable to resolve {host}: {'; '.join(errors)}")


def main():
    # Emit hosts records only. The workflow appends them on its disposable runner;
    # HTTPS certificate verification and apt package signatures stay enabled.
    records = []
    for host in ("archive.ubuntu.com", "security.ubuntu.com"):
        records.extend(f"{address} {host}" for address in resolve(host))
    print("\n".join(records))


if __name__ == "__main__":
    main()
