# The independent emulator CI replays our ESC/POS through
# (docs/implementation/ref/hardware-and-receipts.md §6b.8): one exact emupos,
# installed from hash-locked wheels on a digest-pinned base. Built by CI's
# printer-conformance job, never published, never on a register, never in a
# release.
FROM python:3.13-slim@sha256:cf50a4f6ac4f026458b08d5222023d700ac806410dff5e9ca8728fa44f2b850f

ENV PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_NO_CACHE_DIR=1 \
    PYTHONDONTWRITEBYTECODE=1

COPY requirements.txt /opt/emupos/requirements.txt
# --no-deps: the lock already names every dependency, so nothing is resolved
# at build time; --only-binary: no package builds from source.
RUN pip install --require-hashes --only-binary=:all: --no-deps \
        -r /opt/emupos/requirements.txt \
 && useradd --system --uid 10001 --create-home --home-dir /var/lib/emupos emupos

COPY emupos.yaml /opt/emupos/emupos.yaml
USER emupos
EXPOSE 8765 9100
CMD ["emupos", "run", "--config", "/opt/emupos/emupos.yaml"]
