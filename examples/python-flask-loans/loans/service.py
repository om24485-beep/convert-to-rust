from datetime import date
from decimal import Decimal
from typing import Dict, Optional

from .emi import amortization, emi
from .models import Loan, LoanApplication, LoanStatus

MAX_FOIR = Decimal("0.5")  # fixed obligation to income ratio
MIN_SCORE = 700


class LoanService:
    def __init__(self):
        self.loans: Dict[int, Loan] = {}
        self.next_id = 1

    def evaluate(self, application: LoanApplication) -> dict:
        payment = emi(application.principal, application.annual_rate, application.tenure_months)
        foir = payment / application.customer.monthly_income
        if application.customer.credit_score < MIN_SCORE:
            status, reason = LoanStatus.REJECTED, "credit score below 700"
        elif foir > MAX_FOIR:
            status, reason = LoanStatus.REJECTED, "EMI exceeds 50% of income"
        else:
            status, reason = LoanStatus.APPROVED, None
        loan = Loan(self.next_id, application, status, reason=reason)
        if status == LoanStatus.APPROVED:
            loan.schedule = amortization(application.principal, application.annual_rate, application.tenure_months, date.today())
        self.loans[loan.loan_id] = loan
        self.next_id += 1
        return {"loan_id": loan.loan_id, "status": status.value, "emi": str(payment), "reason": reason}

    def find(self, loan_id: int) -> Optional[Loan]:
        return self.loans.get(loan_id)

    def schedule(self, loan_id: int) -> list:
        loan = self.find(loan_id)
        return [] if loan is None else loan.schedule

    def update_status(self, loan_id: int, status: LoanStatus) -> Loan:
        loan = self.loans[loan_id]
        if loan.status == LoanStatus.REJECTED:
            raise ValueError("rejected loans cannot change status")
        loan.status = status
        return loan
