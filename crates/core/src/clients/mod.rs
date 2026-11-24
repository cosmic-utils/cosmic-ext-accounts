#![allow(dead_code)]

mod account;
mod calendar;
mod contacts;
mod credentials;
mod mail;
mod tasks;

pub use account::AccountsClient;
pub use calendar::{CalendarClient, CalendarCredentials};
pub use contacts::{ContactsClient, ContactsCredentials};
pub use mail::{MailClient, MailCredentials};
pub use tasks::{TasksClient, TasksCredentials};
