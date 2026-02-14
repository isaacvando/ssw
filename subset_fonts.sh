# Subset font files to decrease their size
fonts=(
  "static/fonts/JetBrainsMono-Regular.woff2"
  "static/fonts/JetBrainsMono-Bold.woff2"
  "static/fonts/JetBrainsMono-Italic.woff2"
  "static/fonts/JetBrainsMono-BoldItalic.woff2"
  "static/fonts/JetBrainsMono-ExtraBold.woff2"
)

for font in "${fonts[@]}"; do
  filename=$(basename "$font")
  pyftsubset "$font" \
    --output-file="static/fonts/subset/$filename" \
    --flavor=woff2 \
    --unicodes="U+0020-007E" \
    --layout-features="" \
    --no-hinting \
    --obfuscate-names    
  echo "Optimized $filename"
done
