#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(u64);

impl ConnectionId {
    pub const PRIMARY: Self = Self(1);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::PRIMARY
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionId;

    #[test]
    fn primary_connection_is_default() {
        assert_eq!(ConnectionId::default(), ConnectionId::PRIMARY,);
    }

    #[test]
    fn preserves_connection_value() {
        let id = ConnectionId::new(17);

        assert_eq!(id.value(), 17);
        assert_eq!(id.to_string(), "17");
    }
}
