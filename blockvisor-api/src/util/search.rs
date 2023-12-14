use displaydoc::Display;
use thiserror::Error;

use crate::grpc::common;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Unknown SearchOperator.
    UnknownSearchOperator,
    /// Unknown SortOrder.
    UnknownSortOrder,
}

#[derive(Clone, Copy, Debug)]
pub enum SearchOperator {
    Or,
    And,
}

impl TryFrom<common::SearchOperator> for SearchOperator {
    type Error = Error;

    fn try_from(operator: common::SearchOperator) -> Result<Self, Self::Error> {
        match operator {
            common::SearchOperator::Unspecified => Err(Error::UnknownSearchOperator),
            common::SearchOperator::Or => Ok(SearchOperator::Or),
            common::SearchOperator::And => Ok(SearchOperator::And),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl TryFrom<common::SortOrder> for SortOrder {
    type Error = Error;

    fn try_from(order: common::SortOrder) -> Result<Self, Self::Error> {
        match order {
            common::SortOrder::Unspecified => Err(Error::UnknownSortOrder),
            common::SortOrder::Ascending => Ok(SortOrder::Asc),
            common::SortOrder::Descending => Ok(SortOrder::Desc),
        }
    }
}
