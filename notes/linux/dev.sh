#!/bin/bash

# Abortar o script em caso de erro
set -e

echo "🚀 Iniciando configuração do Amadeuz Notes no Pop!_OS..."

# 1. Instalar dependências do sistema
echo "📦 Instalando dependências via APT..."
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    meson \
    rustc \
    cargo \
    libadwaita-1-dev \
    libsecret-1-dev \
    libglib2.0-dev \
    libgtk-4-dev \
    libgraphene-1.0-dev \
    libxml2-utils \
    blueprint-compiler

# 2. Ajustar Cargo.toml para compatibilidade com Pop!_OS (GNOME 46 / Libadwaita 1.5)
echo "📝 Ajustando versões no Cargo.toml para compatibilidade..."
if [[ -f "Cargo.toml" ]]; then
    # Substitui gnome_47 por gnome_46 e v1_7/v1_6 por v1_5
    sed -i 's/features = \["gnome_47"\]/features = ["gnome_46"]/g' Cargo.toml
    sed -i 's/features = \["v1_7"\]/features = ["v1_5"]/g' Cargo.toml
    sed -i 's/features = \["v1_6"\]/features = ["v1_5"]/g' Cargo.toml
    echo "✅ Cargo.toml atualizado."
else
    echo "❌ Erro: Cargo.toml não encontrado!"
    exit 1
fi

# 3. Limpar builds anteriores
echo "🧹 Limpando ambiente de build..."
rm -rf build/

# 4. Configurar Meson com prefixo local
echo "⚙️ Configurando Meson..."
meson setup build --prefix=$HOME/.local -Dprofile=development

# 5. Compilar e Instalar localmente
echo "🔨 Compilando projeto..."
ninja -C build
meson install -C build

# 6. Configurar GSettings Schemas
echo "🗂️ Configurando Schemas do GSettings..."
mkdir -p ~/.local/share/glib-2.0/schemas
cp data/com.amadeuz.Notes.gschema.xml ~/.local/share/glib-2.0/schemas/
glib-compile-schemas ~/.local/share/glib-2.0/schemas/

echo ""
echo "✨ Tudo pronto! Para rodar o aplicativo, use:"
echo "GSETTINGS_SCHEMA_DIR=~/.local/share/glib-2.0/schemas ~/.local/bin/amadeuz-notes"