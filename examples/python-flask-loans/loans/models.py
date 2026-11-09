from dataclasses import dataclass, field
from datetime import date
from decimal import Decimal
from enum import Enum
from typing import List, Optional


class LoanStatus(Enum):
    APPLIED = "applied"
    APPROVED = "approved"
    REJECTED = "rejected"
    DISBURSED = "disbursed"


@dataclass
class Customer:
    customer_id: str
    full_name: str
    monthly_income: Decimal
    credit_score: int
    pan: Optional[str] = None


@dataclass
class LoanApplication:
    customer: Customer
    principal: Decimal
    annual_rate: Decimal
    tenure_months: int
    purpose: str = "personal"


@dataclass
class Installment:
    number: int
    due_date: date
    emi: Decimal
    principal_part: Decimal
    interest_part: Decimal
    balance: Decimal


@dataclass
class Loan:
    loan_id: int
    application: LoanApplication
    status: LoanStatus
    schedule: List[Installment] = field(default_factory=list)
    reason: Optional[str] = None
