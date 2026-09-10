//! File: crates/runen-ecs/src/reflect/enum_info.rs
//! Purpose: Reflected no-payload enum metadata.

pub type EnumCurrentVariant = fn(&dyn std::any::Any) -> Option<&'static str>;
pub type EnumSetUnitVariant = fn(&mut dyn std::any::Any, &str) -> bool;
pub type EnumVariantAt = fn(usize) -> Option<EnumVariantInfo>;

#[derive(Debug, Clone, Copy)]
pub struct EnumVariantInfo {
    pub symbol: &'static str,
    pub display_name: &'static str,
}

impl EnumVariantInfo {
    pub const fn new(symbol: &'static str, display_name: &'static str) -> Self {
        Self {
            symbol,
            display_name,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EnumInfo {
    variant_count: usize,
    variant_at: EnumVariantAt,
    pub current_variant: EnumCurrentVariant,
    pub set_unit_variant: EnumSetUnitVariant,
}

impl EnumInfo {
    pub const fn new(
        variant_count: usize,
        variant_at: EnumVariantAt,
        current_variant: EnumCurrentVariant,
        set_unit_variant: EnumSetUnitVariant,
    ) -> Self {
        Self {
            variant_count,
            variant_at,
            current_variant,
            set_unit_variant,
        }
    }

    pub fn variant_count(&self) -> usize {
        self.variant_count
    }

    pub fn variants(&self) -> Vec<EnumVariantInfo> {
        (0..self.variant_count)
            .filter_map(|index| (self.variant_at)(index))
            .collect()
    }

    pub fn variant_named(&self, symbol: &str) -> Option<EnumVariantInfo> {
        self.variants()
            .into_iter()
            .find(|variant| variant.symbol == symbol)
    }
}
