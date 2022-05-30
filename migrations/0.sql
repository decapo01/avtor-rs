
CREATE table if not exists migrations (
  id uuid not null primary key,
  name varchar(255) not null,
  seq_order int not null,
  up text not null,
  down text not null,
  applied_on timestamp default current_timestamp 
);