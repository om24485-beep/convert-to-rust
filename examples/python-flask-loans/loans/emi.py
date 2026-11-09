from datetime import date
from decimal import Decimal, ROUND_HALF_UP

from .models import Installment

TWO_PLACES = Decimal("0.01")


def monthly_rate(annual_rate: Decimal) -> Decimal:
    return annual_rate / Decimal(12) / Decimal(100)


def emi(principal: Decimal, annual_rate: Decimal, tenure_months: int) -> Decimal:
    """Equated monthly instalment, rounded to paise."""
    r = monthly_rate(annual_rate)
    if r == 0:
        return (principal / tenure_months).quantize(TWO_PLACES, ROUND_HALF_UP)
    factor = (1 + r) ** tenure_months
    value = principal * r * factor / (factor - 1)
    return value.quantize(TWO_PLACES, ROUND_HALF_UP)


def amortization(principal: Decimal, annual_rate: Decimal, tenure_months: int, start: date) -> list:
    r = monthly_rate(annual_rate)
    payment = emi(principal, annual_rate, tenure_months)
    balance = principal
    rows = []
    for n in range(1, tenure_months + 1):
        interest = (balance * r).quantize(TWO_PLACES, ROUND_HALF_UP)
        principal_part = payment - interest
        balance = max(balance - principal_part, Decimal(0))
        month = (start.month - 1 + n) % 12 + 1
        year = start.year + (start.month - 1 + n) // 12
        rows.append(Installment(n, date(year, month, 1), payment, principal_part, interest, balance))
    return rows
