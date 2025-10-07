use bitfield_struct::bitfield;
use colored::Colorize;
use std::fmt::Display;

/// Custom 4-byte packet structure used for controlling LED
/// state through a LiveKit room.
#[bitfield(u32)]
pub struct LedControlPacket {
    /// Packet version (0-4).
    #[bits(2)]
    pub version: u8,
    /// Which LED is being controlled (0-15).
    #[bits(5)]
    pub channel: u8,
    /// Whether or not the channel is on.
    #[bits(1)]
    pub is_on: bool,
    /// Red intensity (0-255).
    #[bits(8)]
    pub red: u8,
    /// Green intensity (0-255).
    #[bits(8)]
    pub green: u8,
    /// Blue intensity (0-255).
    #[bits(8)]
    pub blue: u8,
}

impl Display for LedControlPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let color_display = if colored::control::SHOULD_COLORIZE.should_colorize() {
            "   ".on_truecolor(self.red(), self.green(), self.blue())
        } else {
            // Display RGB value if terminal color is disabled.
            format!("rgb({:>3}, {:>3}, {:>3})", self.red(), self.green(), self.blue()).into()
        };
        write!(f, "Channel {:02} => {}", self.channel(), color_display)
    }
}

#[cfg(test)]
mod tests {
    use super::LedControlPacket;

    #[test]
    fn test_bit_representation() {
        let packet = LedControlPacket::new()
            .with_version(1)
            .with_channel(4)
            .with_is_on(true)
            .with_red(31)
            .with_green(213)
            .with_blue(249);
        assert_eq!(packet.into_bits(), 0xF9D51F91);
    }
}
