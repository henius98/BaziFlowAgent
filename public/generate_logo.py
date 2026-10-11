import math


def generate_svg():
    svg = []
    svg.append(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 800" width="800" height="800">')
    svg.append('  <defs>')
    svg.append('    <radialGradient id="bg" cx="50%" cy="50%" r="50%">')
    svg.append('      <stop offset="0%" stop-color="#1e293b"/>')
    svg.append('      <stop offset="100%" stop-color="#020617"/>')
    svg.append('    </radialGradient>')
    svg.append(
        '    <filter id="glow-gold" x="-20%" y="-20%" width="140%" height="140%">')
    svg.append('      <feGaussianBlur stdDeviation="5" result="blur"/>')
    svg.append('      <feMerge>')
    svg.append('        <feMergeNode in="blur"/>')
    svg.append('        <feMergeNode in="SourceGraphic"/>')
    svg.append('      </feMerge>')
    svg.append('    </filter>')
    svg.append(
        '    <filter id="glow-cyan" x="-20%" y="-20%" width="140%" height="140%">')
    svg.append('      <feGaussianBlur stdDeviation="5" result="blur"/>')
    svg.append('      <feMerge>')
    svg.append('        <feMergeNode in="blur"/>')
    svg.append('        <feMergeNode in="SourceGraphic"/>')
    svg.append('      </feMerge>')
    svg.append('    </filter>')
    svg.append('  </defs>')

    svg.append('  <rect width="800" height="800" fill="url(#bg)"/>')

    # Grid/Circuit aesthetic lines
    svg.append('  <g stroke="#1e293b" stroke-width="1.5" opacity="0.8">')
    for i in range(0, 800, 30):
        svg.append(f'    <line x1="{i}" y1="0" x2="{i}" y2="800" />')
        svg.append(f'    <line x1="0" y1="{i}" x2="800" y2="{i}" />')
    svg.append('  </g>')

    svg.append('  <g transform="translate(400, 400)">')

    # Outer Tech Circles
    svg.append('    <circle cx="0" cy="0" r="360" fill="none" stroke="#00ffff" stroke-width="3" filter="url(#glow-cyan)" opacity="0.3"/>')
    svg.append('    <circle cx="0" cy="0" r="345" fill="none" stroke="#00ffff" stroke-width="1.5" opacity="0.8" stroke-dasharray="10 6 2 6"/>')
    svg.append('    <circle cx="0" cy="0" r="185" fill="none" stroke="#FFD700" stroke-width="2" filter="url(#glow-gold)" opacity="0.5" stroke-dasharray="20 10"/>')

    # AI Circuit Traces
    svg.append(
        '    <g stroke="#00ffff" stroke-width="2" fill="none" opacity="0.7" filter="url(#glow-cyan)">')
    import random
    random.seed(101)
    for _ in range(16):
        angle = random.uniform(0, math.pi * 2)
        r1 = random.uniform(190, 210)
        r2 = random.uniform(280, 300)
        r3 = random.uniform(320, 340)

        x1, y1 = math.cos(angle) * r1, math.sin(angle) * r1
        x2, y2 = math.cos(angle) * r2, math.sin(angle) * r2
        angle_offset = random.choice([-0.15, 0.15])
        x3, y3 = math.cos(angle + angle_offset) * \
            r3, math.sin(angle + angle_offset) * r3

        svg.append(
            f'      <polyline points="{x1},{y1} {x2},{y2} {x3},{y3}" />')
        svg.append(
            f'      <circle cx="{x3}" cy="{y3}" r="4" fill="#00ffff" />')
    svg.append('    </g>')

    # Binary Ring
    svg.append(
        '    <g fill="#00ffff" opacity="0.4" font-family="monospace" font-size="12" filter="url(#glow-cyan)">')
    for angle_deg in range(0, 360, 10):
        rad = math.radians(angle_deg)
        x = math.cos(rad) * 375
        y = math.sin(rad) * 375
        binary_str = "".join([str(random.randint(0, 1)) for _ in range(4)])
        svg.append(
            f'      <text x="{x}" y="{y}" transform="rotate({angle_deg + 90}, {x}, {y})" text-anchor="middle">{binary_str}</text>')
    svg.append('    </g>')

    # AI Core Labels
    svg.append('    <g font-family="sans-serif" font-weight="bold" font-size="14" fill="#FFD700" opacity="0.8" filter="url(#glow-gold)">')
    svg.append('    </g>')

    # Yin-Yang (Strictly adhering to reference)
    svg.append(
        '    <circle cx="0" cy="0" r="150" fill="none" stroke="#FFD700" stroke-width="4" filter="url(#glow-gold)"/>')

    # White part (left side, head at top)
    svg.append('    <path d="M 0 -150 A 150 150 0 0 0 0 150 A 75 75 0 0 1 0 0 A 75 75 0 0 0 0 -150 Z" fill="#f8fafc" stroke="#00ffff" stroke-width="1" filter="url(#glow-cyan)"/>')
    # Black part (right side, head at bottom)
    svg.append('    <path d="M 0 150 A 150 150 0 0 0 0 -150 A 75 75 0 0 1 0 0 A 75 75 0 0 0 0 150 Z" fill="#020617" stroke="#FFD700" stroke-width="1" filter="url(#glow-gold)"/>')

    # Dots
    svg.append(
        '    <circle cx="0" cy="-75" r="24" fill="#020617" filter="url(#glow-cyan)"/>')
    svg.append(
        '    <circle cx="0" cy="75" r="24" fill="#f8fafc" filter="url(#glow-gold)"/>')

    # Trigrams (King Wen / Late Heaven)
    # sequence: Top, Top-Left, Left, Bottom-Left, Bottom, Bottom-Right, Right, Top-Right
    # 1=solid, 0=broken. Inner, Middle, Outer
    bagua = [
        (1, 0, 1),  # 0 deg (Top) -> Li ☲
        (0, 1, 1),  # -45 / 315 deg (Top-Left) -> Xun ☴
        (1, 0, 0),  # -90 / 270 deg (Left) -> Zhen ☳
        (0, 0, 1),  # -135 / 225 deg (Bottom-Left) -> Gen ☶
        (0, 1, 0),  # 180 deg (Bottom) -> Kan ☵
        (1, 1, 1),  # 135 deg (Bottom-Right) -> Qian ☰
        (1, 1, 0),  # 90 deg (Right) -> Dui ☱
        (0, 0, 0),  # 45 deg (Top-Right) -> Kun ☷
    ]

    angles = [0, -45, -90, -135, 180, 135, 90, 45]

    line_thickness = 20
    line_length = 110
    gap = 14
    radius_start = 200

    for i, (inner, middle, outer) in enumerate(bagua):
        angle = angles[i]
        svg.append(f'    <g transform="rotate({angle})">')

        lines = [inner, middle, outer]
        for j, solid in enumerate(lines):
            r = radius_start + j * (line_thickness + gap)
            if solid:
                # Solid line
                svg.append(
                    f'      <rect x="{-line_length/2}" y="{-r - line_thickness}" width="{line_length}" height="{line_thickness}" fill="#FFD700" rx="4" filter="url(#glow-gold)"/>')
            else:
                # Broken line
                part_length = (line_length - 24) / 2
                svg.append(
                    f'      <rect x="{-line_length/2}" y="{-r - line_thickness}" width="{part_length}" height="{line_thickness}" fill="#00ffff" rx="4" filter="url(#glow-cyan)"/>')
                svg.append(
                    f'      <rect x="{line_length/2 - part_length}" y="{-r - line_thickness}" width="{part_length}" height="{line_thickness}" fill="#00ffff" rx="4" filter="url(#glow-cyan)"/>')

        svg.append('    </g>')

    svg.append('  </g>')
    svg.append('</svg>')

    with open('logo_late_heaven.svg', 'w') as f:
        f.write('\n'.join(svg))


if __name__ == "__main__":
    generate_svg()
