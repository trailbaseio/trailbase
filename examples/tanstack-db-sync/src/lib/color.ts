// Function to generate a complementary color
export function getComplementaryColor(hexColor: string | undefined): string {
  // Default to a nice blue if no color is provided
  if (!hexColor) return `#3498db`;

  // Remove the hash if it exists
  const color = hexColor.replace(/^#/, ``);

  // Convert hex to RGB
  const r = parseInt(color.substring(0, 2), 16);
  const g = parseInt(color.substring(2, 2), 16);
  const b = parseInt(color.substring(4, 2), 16);

  // Calculate complementary color (inverting the RGB values)
  const compR = 255 - r;
  const compG = 255 - g;
  const compB = 255 - b;

  // Calculate brightness of the background
  const brightness = r * 0.299 + g * 0.587 + b * 0.114;

  // If the complementary color doesn't have enough contrast, adjust it
  const compBrightness = compR * 0.299 + compG * 0.587 + compB * 0.114;
  const brightnessDiff = Math.abs(brightness - compBrightness);

  if (brightnessDiff < 128) {
    // Not enough contrast, use a more vibrant alternative
    if (brightness > 128) {
      // Dark color for light background
      return `#8e44ad`; // Purple
    } else {
      // Light color for dark background
      return `#f1c40f`; // Yellow
    }
  }

  // Convert back to hex
  return `#${((1 << 24) + (compR << 16) + (compG << 8) + compB).toString(16).slice(1)}`;
}
