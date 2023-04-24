pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pub async fn select2<A, B>(left: &mut A, right: &mut B) -> Either<A::Output, B::Output>
where
    A: std::future::Future + Send + Sync + Unpin,
    B: std::future::Future + Send + Sync + Unpin,
{
    tokio::select! {
        left = left => Either::Left(left),
        right = right => Either::Right(right),
    }
}
