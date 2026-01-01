//! Conversion implementations between database types and proto types for contacts backup.

use super::{AddressData, Email, FullContact, PhoneNumber, Url, WalletAddress};
use xmtp_proto::xmtp::device_sync::contact_backup::{
    AddressSave, ContactSave, EmailSave, PhoneNumberSave, UrlSave, WalletAddressSave,
};

impl From<FullContact> for ContactSave {
    fn from(contact: FullContact) -> Self {
        Self {
            inbox_id: contact.inbox_id,
            display_name: contact.display_name,
            first_name: contact.first_name,
            last_name: contact.last_name,
            prefix: contact.prefix,
            suffix: contact.suffix,
            company: contact.company,
            job_title: contact.job_title,
            birthday: contact.birthday,
            note: contact.note,
            image_url: contact.image_url,
            is_favorite: contact.is_favorite,
            created_at_ns: contact.created_at_ns,
            updated_at_ns: contact.updated_at_ns,
            phone_numbers: contact.phone_numbers.into_iter().map(Into::into).collect(),
            emails: contact.emails.into_iter().map(Into::into).collect(),
            urls: contact.urls.into_iter().map(Into::into).collect(),
            wallet_addresses: contact
                .wallet_addresses
                .into_iter()
                .map(Into::into)
                .collect(),
            addresses: contact.addresses.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<PhoneNumber> for PhoneNumberSave {
    fn from(p: PhoneNumber) -> Self {
        Self {
            phone_number: p.phone_number,
            label: p.label,
        }
    }
}

impl From<Email> for EmailSave {
    fn from(e: Email) -> Self {
        Self {
            email: e.email,
            label: e.label,
        }
    }
}

impl From<Url> for UrlSave {
    fn from(u: Url) -> Self {
        Self {
            url: u.url,
            label: u.label,
        }
    }
}

impl From<WalletAddress> for WalletAddressSave {
    fn from(w: WalletAddress) -> Self {
        Self {
            wallet_address: w.wallet_address,
            label: w.label,
        }
    }
}

impl From<AddressData> for AddressSave {
    fn from(a: AddressData) -> Self {
        Self {
            address1: a.address1,
            address2: a.address2,
            address3: a.address3,
            city: a.city,
            region: a.region,
            postal_code: a.postal_code,
            country: a.country,
            label: a.label,
        }
    }
}

impl From<AddressSave> for AddressData {
    fn from(a: AddressSave) -> Self {
        Self {
            id: None,
            address1: a.address1,
            address2: a.address2,
            address3: a.address3,
            city: a.city,
            region: a.region,
            postal_code: a.postal_code,
            country: a.country,
            label: a.label,
        }
    }
}
