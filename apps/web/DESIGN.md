---
name: Tessera
description: Anonymous file courier with fast, frictionless ephemeral sharing
colors:
  primary: "hsl(var(--primary))"
  primary-foreground: "hsl(var(--primary-foreground))"
  secondary: "hsl(var(--secondary))"
  secondary-foreground: "hsl(var(--secondary-foreground))"
  destructive: "hsl(var(--destructive))"
  muted: "hsl(var(--muted))"
  muted-foreground: "hsl(var(--muted-foreground))"
  accent: "hsl(var(--accent))"
  accent-foreground: "hsl(var(--accent-foreground))"
  card: "hsl(var(--card))"
  card-foreground: "hsl(var(--card-foreground))"
  input: "hsl(var(--input))"
  border: "hsl(var(--border))"
  ring: "hsl(var(--ring))"
  background: "hsl(var(--background))"
  foreground: "hsl(var(--foreground))"
typography:
  display:
    fontFamily: "Inter Variable, Inter, sans-serif"
    fontSize: "clamp(1.75rem, 5vw, 2.25rem)"
    fontWeight: 600
    lineHeight: 1.2
  heading:
    fontFamily: "Inter Variable, Inter, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 600
    lineHeight: 1.4
  title:
    fontFamily: "Inter Variable, Inter, sans-serif"
    fontSize: "1rem"
    fontWeight: 500
    lineHeight: 1.5
  body:
    fontFamily: "Inter Variable, Inter, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Inter Variable, Inter, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.5
    letterSpacing: "0.05em"
rounded:
  sm: "4px"
  md: "8px"
  lg: "8px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
  xl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-foreground}"
    rounded: "{rounded.md}"
    padding: "6px 8px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.foreground}"
    rounded: "{rounded.md}"
    padding: "6px 8px"
  button-link:
    backgroundColor: "transparent"
    textColor: "{colors.primary}"
    rounded: "{rounded.md}"
    padding: "6px 8px"
  card-default:
    backgroundColor: "{colors.card}"
    textColor: "{colors.card-foreground}"
    rounded: "{rounded.lg}"
    padding: "{spacing.md}"
  input-default:
    backgroundColor: "{colors.input}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.md}"
    padding: "6px 8px"
---

# Design System: Tessera

## Overview

**Creative North Star: "Fast & Frictionless"**

Tessera's interface prioritizes speed and ease above all else. Every interaction removes friction: uploads take seconds, no account required, sharing is instant. The visual system reflects this velocity—clean, minimal, and purposeful. Motion and selective color accents create moments of delight without cluttering the core flow. Typography and spacing follow a tight rhythm that guides the eye without decoration. The overall aesthetic is modern and minimal, with professional polish and careful attention to responsive behavior across all devices.

**Key Characteristics:**

- Minimal baseline with selective delight in key moments (animations, color accents)
- Fast-loading, responsive-first design
- Light and dark theme support with theme-aware colors
- Smooth transitions and purposeful animations
- High contrast and clear hierarchy for readability
- Touch-friendly on mobile, optimized for keyboard navigation

## Colors

A neutral-forward palette with a single primary accent and supporting semantic colors for errors and secondary actions. Colors are defined as CSS custom properties and adapt to light and dark themes automatically.

### Primary

- **Primary Accent** (`hsl(var(--primary))`): The main action color used for primary buttons, key CTAs, and interactive highlights. Used sparingly (≤10% of screen) to maintain focus.

### Secondary

- **Secondary Accent** (`hsl(var(--secondary))`): Used for secondary actions, filters, or supporting UI elements that need distinction but lower priority than primary.

### Neutral

- **Background** (`hsl(var(--background))`): Page and surface background.
- **Card** (`hsl(var(--card))`): Card and container backgrounds, distinct from page background via subtle elevation.
- **Foreground** (`hsl(var(--foreground))`): Primary text color for body copy and UI labels.
- **Muted** (`hsl(var(--muted))`): Secondary surfaces and hover backgrounds (ghost buttons, etc.).
- **Muted Foreground** (`hsl(var(--muted-foreground))`): Secondary text for descriptions, hints, and de-emphasized content.
- **Border** (`hsl(var(--border))`): Subtle dividing lines and card borders, used at reduced opacity (ring-foreground/10).
- **Input** (`hsl(var(--input))`): Input field backgrounds and interactive controls.
- **Ring** (`hsl(var(--ring))`): Focus outline color, used with opacity for accessibility.

### Semantic

- **Destructive** (`hsl(var(--destructive))`): Error states, warnings, and destructive actions. Used in error messages and delete confirmations.
- **Accent** (`hsl(var(--accent))`): Supplementary highlight color for charts, badges, or special states.

### Named Rule

**The One Voice Rule.** The primary accent is used sparingly across any screen. Its rarity makes it authoritative when it appears. Secondary actions use ghost or outline variants to maintain visual quietness.

## Typography

**Display Font:** Inter Variable (with Inter fallback) **Scale Ratio:** Consistent sizing across display, heading, title, body, and label roles

**Character:** Modern, clean, and utilitarian. Inter's neutral personality suits Tessera's focus on content over decoration. The typeface supports accessibility through clear letterforms and excellent readability at all sizes.

### Hierarchy

- **Display** (600 weight, clamp(1.75rem, 5vw, 2.25rem), 1.2 line-height): Hero headlines and primary page titles. Used sparingly; most pages don't need this.
- **Heading** (600 weight, 1.25rem, 1.4 line-height): Section headers, modal titles. Strong visual weight to guide navigation.
- **Title** (500 weight, 1rem, 1.5 line-height): Card titles, form labels, and subsection headers. Medium emphasis.
- **Body** (400 weight, 0.875rem, 1.5 line-height): Primary content, descriptions, and body text. Optimized for reading; line-height of 1.5 aids readability.
- **Label** (500 weight, 0.75rem, 1.5 line-height, 0.05em letter-spacing): UI labels, badges, and small text. Slightly letter-spaced for clarity at small sizes.

### Named Rules

**The Clarity Rule.** Font size and weight are chosen for scanning and readability, not decoration. Every size step serves a functional purpose in the hierarchy.

## Layout

Tessera uses a flexible, responsive grid system built on Tailwind's spacing scale. Spacing follows a consistent rhythm: 4px (xs), 8px (sm), 16px (md), 24px (lg), 32px (xl). Cards and containers use a CSS custom property `--card-spacing` (default 16px, reduced to 12px on small variants) to unify internal padding.

**Responsive Behavior:** The interface is mobile-first and works equally well on all screen sizes. Button sizes, typography scale via clamp(), and spacing adjust naturally without media-query breakpoints where possible. Touch targets are always ≥ 24px (except icons at 20px minimum).

**Density:** Comfortable default spacing with no overcrowding. Inputs, buttons, and cards maintain readable line-heights and visual breathing room.

## Elevation & Depth

Tessera uses minimal shadows and prefers tonal layering. Depth is conveyed primarily through:

- **Tonal differentiation**: Card backgrounds are distinct from page background via the `card` color token.
- **Borders**: Subtle borders (border-foreground/10 opacity) separate surfaces without visual noise.
- **Focus rings**: Focus states use a 2px ring with ring/30 opacity to indicate interactive focus without aggressive visual weight.

### Shadow Vocabulary

- **No default shadows.** The interface is flat by default, relying on color and borders for depth. Hover states may use subtle color shifts (opacity changes via color-mix or hover:bg-opacity) rather than shadows.
- **Optional: Animated accents.** Glass-background gradients (e.g., primary/35 with blur-[110px]) add visual interest without structural shadows.

## Shapes

Rounded corners are used consistently but restfully. Primary radius is 8px (md), applied to buttons, cards, inputs, and dialogs. Smaller components may use 4px (sm). No fully rounded elements or extreme curves; corners support the modern, functional aesthetic.

**Corner Strategy:** 8px radius on containers and interactive elements creates a friendly, approachable feel without softness. Card images respect rounded corners (rounded-t-lg on first image, rounded-b-lg on last).

## Components

### Buttons

- **Shape:** 8px rounded corners (md).
- **Default variant:** Primary accent background with primary-foreground text. Solid fill, no border.
- **Ghost variant:** Transparent background with muted hover state. Used for secondary actions and less prominent CTAs.
- **Link variant:** Underlined text in primary color. Used for inline actions and navigation.
- **Destructive variant:** Reduced opacity destructive background (10% in light, 20% in dark) with red text. Hover increases opacity. Used for risky actions.
- **Outline variant:** Subtle border, light hover background. Used for filters or secondary toggles.
- **Secondary variant:** Secondary-foreground text on secondary background. Hover via color-mix.
- **Sizes:** xs (20px height), sm (24px), default (28px), lg (32px), icon sizes (20–32px per variant). Padding scales with size; all include icon support with automatic gutter.
- **States:** Hover applies opacity shift (primary/80) or background color change. Focus adds a 2px ring (ring/30 opacity). Active adds a subtle downward translate (1px). Disabled reduces opacity to 50%.

### Inputs & Text Fields

- **Style:** Light background (input/20 in light mode, input/30 in dark), subtle border (border-input). 8px radius, 6px vertical padding, 8px horizontal.
- **Focus:** Border becomes ring color, adds 2px ring at ring/30 opacity. Smooth transition on focus.
- **Error:** Border and ring become destructive color (destructive/40 to 50% opacity). Applied via aria-invalid state.
- **Placeholder:** Muted-foreground color for accessible placeholder text.
- **File input:** Styled file button with foreground text, no visible file input styling.

### Cards

- **Background:** Card color, distinct from page background.
- **Padding:** Default 16px internal spacing (sm variant uses 12px).
- **Border:** ring-foreground/10 (subtle, 1px stroke-width via ring-1 utility).
- **Radius:** 8px corners.
- **Subcomponents:** Header (grid-based with auto-rows), Title (font-medium, sm size), Description (muted text, xs size), Content (flexible), Footer (flex row for actions). Images at top or bottom of card receive rounded corners (rounded-t-lg / rounded-b-lg).
- **Responsive:** Adjusts smoothly on smaller screens; no layout collapse.

### Navigation (Header)

- **Fixed position** at top with z-50 to sit above content.
- **Flex layout** with justify-end and gap-2 (8px spacing).
- **Theme toggle:** Animated dark/light mode switcher.
- **Locale selector:** Dropdown to switch language at runtime.
- **Mobile-first:** All controls remain touch-accessible on small screens; no mobile menu needed for these minimal controls.

## Do's and Don'ts

### Do:

- **Do** use primary accent sparingly (≤10% of screen). Every instance of primary color is a visual call-to-action.
- **Do** apply smooth transitions on state changes (hover, focus, active). Keeps the interface feeling responsive.
- **Do** ensure focus rings are visible on all interactive elements for keyboard navigation.
- **Do** use the card component for content grouping; it provides visual separation without visual noise.
- **Do** test on mobile and tablet to verify touch targets are ≥ 24px and spacing is comfortable.
- **Do** prefer ghost and outline button variants for secondary actions; primary should be rare.
- **Do** use the 8px baseline radius consistently; don't mix sharp corners with fully rounded elements.

### Don't:

- **Don't** add drop shadows to create depth; use tonal differences and borders instead.
- **Don't** use more than one or two accent colors on a single screen.
- **Don't** add decorative elements that don't serve a functional purpose.
- **Don't** forget keyboard navigation; every interactive element must have a clear focus state.
- **Don't** set text sizes in fixed pixels; use rem or clamp() for responsive scaling.
- **Don't** ignore dark mode; test both themes to ensure colors have sufficient contrast.
- **Don't** overcrowd spacing; let content breathe with consistent margins and padding.
