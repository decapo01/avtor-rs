use std::future::Future;

pub async fn create<T, DTO, E, FA, FB>(
    validate: impl FnOnce(&DTO) -> Result<(), E>,
    find_unique: impl FnOnce(&T) -> FA,
    insert: impl FnOnce(&T) -> FB,
    mapper: impl FnOnce(&DTO) -> T,
    dto: &DTO,
    e: E,
) -> Result<(), E>
where
    FA: Future<Output = Result<Option<T>, E>>,
    FB: Future<Output = Result<(), E>>,
{
    let _ = validate(dto)?;
    let item = mapper(dto);
    let maybe_existing = find_unique(&item).await?;
    match maybe_existing {
        Some(_) => Err(e),
        None => insert(&item).await,
    }
}
