# Pinned by digest (not just tag) so the devcontainer manifest digest stays
# stable across rebuilds. Microsoft rebuilds the `ubuntu-24.04` tag
# periodically (for security patches), and an unpinned tag would silently
# pull new layers and break `.devcontainer/devcontainer.json`'s digest check.
# Bump this digest (and re-run the `set-devcontainer-image` flow) to pick up
# upstream security updates.
FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04@sha256:d94c97dd9cacf183d0a6fd12a8e87b526e9e928307674ae9c94139139c0c6eae

# Ensure vscode user exists (base image should have it, but some runtimes need it explicit)
RUN if ! id vscode 2>/dev/null; then \
        groupadd --gid 1000 vscode \
        && useradd --uid 1000 --gid 1000 -m -s /bin/zsh vscode \
        && echo 'vscode ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers.d/vscode \
        && chmod 0440 /etc/sudoers.d/vscode; \
    fi

RUN apt-get update \
    && apt-get install -y --no-install-recommends docker.io docker-compose-v2 \
    && curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && usermod -aG docker vscode \
    && chsh -s /bin/zsh vscode

RUN mkdir -p /nix && chown vscode:vscode /nix

USER vscode
ENV USER=vscode

RUN curl -L https://nixos.org/nix/install | sh -s -- --no-daemon

COPY --chown=vscode:vscode nix.conf /home/vscode/.config/nix/nix.conf

# ENV makes nix available in subsequent RUN layers; sourcing nix.sh sets additional env vars for the current RUN
ENV PATH="/home/vscode/.nix-profile/bin:/nix/var/nix/profiles/default/bin:${PATH}"
RUN . /home/vscode/.nix-profile/etc/profile.d/nix.sh \
    && nix-env -iA nixpkgs.direnv nixpkgs.nix-direnv nixpkgs.just

COPY --chown=vscode:vscode direnvrc /home/vscode/.config/direnv/direnvrc

# zshenv is loaded before zshrc, ensuring Nix is on PATH before direnv evaluates
COPY --chown=vscode:vscode zshenv /home/vscode/.zshenv

RUN echo 'eval "$(direnv hook zsh)"' >> /home/vscode/.zshrc
