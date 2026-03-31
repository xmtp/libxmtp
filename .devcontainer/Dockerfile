FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends docker.io docker-compose-v2 \
    && curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && usermod -aG docker vscode

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
